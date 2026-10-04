use crate::eq::{EqState, BAND_FREQUENCIES};
use pipewire::sys;
use std::ffi::CString;
use std::os::raw::c_void;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

const FILTER_NODE: &str = "Harmonic EQ";
const FILTER_SINK: &str = "harmonic_eq";
const SAMPLE_RATE: f32 = 48_000.0;
const Q: f32 = 1.0;

#[derive(Clone, Copy, Default)]
struct Coefficients {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

#[derive(Clone, Copy, Default)]
struct BiquadState {
    z1: f32,
    z2: f32,
}

impl BiquadState {
    #[inline]
    fn process(&mut self, x: f32, c: Coefficients) -> f32 {
        let y = c.b0 * x + self.z1;
        self.z1 = c.b1 * x - c.a1 * y + self.z2;
        self.z2 = c.b2 * x - c.a2 * y;
        y
    }
}

struct AtomicCoefficients {
    values: [AtomicU32; 5],
}

impl AtomicCoefficients {
    fn new(c: Coefficients) -> Self {
        Self {
            values: [
                AtomicU32::new(c.b0.to_bits()),
                AtomicU32::new(c.b1.to_bits()),
                AtomicU32::new(c.b2.to_bits()),
                AtomicU32::new(c.a1.to_bits()),
                AtomicU32::new(c.a2.to_bits()),
            ],
        }
    }

    #[inline]
    fn load(&self) -> Coefficients {
        Coefficients {
            b0: f32::from_bits(self.values[0].load(Ordering::Relaxed)),
            b1: f32::from_bits(self.values[1].load(Ordering::Relaxed)),
            b2: f32::from_bits(self.values[2].load(Ordering::Relaxed)),
            a1: f32::from_bits(self.values[3].load(Ordering::Relaxed)),
            a2: f32::from_bits(self.values[4].load(Ordering::Relaxed)),
        }
    }

    fn store(&self, c: Coefficients) {
        self.values[0].store(c.b0.to_bits(), Ordering::Relaxed);
        self.values[1].store(c.b1.to_bits(), Ordering::Relaxed);
        self.values[2].store(c.b2.to_bits(), Ordering::Relaxed);
        self.values[3].store(c.a1.to_bits(), Ordering::Relaxed);
        self.values[4].store(c.a2.to_bits(), Ordering::Relaxed);
    }
}

struct FilterShared {
    enabled: AtomicBool,
    preamp: AtomicU32,
    bands: [AtomicCoefficients; 31],
    loop_ptr: AtomicPtr<sys::pw_main_loop>,
    ready: AtomicBool,
    failed: AtomicBool,
}

impl FilterShared {
    fn new() -> Self {
        Self {
            enabled: AtomicBool::new(true),
            preamp: AtomicU32::new(1.0f32.to_bits()),
            bands: std::array::from_fn(|_| AtomicCoefficients::new(identity_coefficients())),
            loop_ptr: AtomicPtr::new(ptr::null_mut()),
            ready: AtomicBool::new(false),
            failed: AtomicBool::new(false),
        }
    }

    fn update(&self, state: &EqState) {
        self.enabled.store(state.enabled, Ordering::Relaxed);
        self.preamp.store(
            db_to_linear(state.preamp_db).to_bits(),
            Ordering::Relaxed,
        );

        for (index, band) in state.bands.iter().enumerate() {
            self.bands[index].store(if band.enabled {
                peaking_coefficients(band.frequency, band.gain_db, SAMPLE_RATE, Q)
            } else {
                identity_coefficients()
            });
        }
    }
}

struct NativeFilterHandle {
    shared: Arc<FilterShared>,
    thread: Option<JoinHandle<()>>,
}

impl NativeFilterHandle {
    fn start(state: &EqState, target: &str) -> Result<Self, String> {
        let shared = Arc::new(FilterShared::new());
        shared.update(state);

        let thread_shared = Arc::clone(&shared);
        let target = target.to_owned();

        let thread = thread::Builder::new()
            .name("harmonic-pw-filter".into())
            .spawn(move || run_filter(thread_shared, target))
            .map_err(|e| format!("failed to create PipeWire DSP thread: {e}"))?;

        for _ in 0..100 {
            if shared.ready.load(Ordering::Acquire) {
                return Ok(Self {
                    shared,
                    thread: Some(thread),
                });
            }

            if shared.failed.load(Ordering::Acquire) {
                let _ = thread.join();
                return Err("PipeWire native DSP filter failed to initialize".into());
            }

            thread::sleep(std::time::Duration::from_millis(10));
        }

        let loop_ptr = shared.loop_ptr.load(Ordering::Acquire);
        if !loop_ptr.is_null() {
            unsafe {
                let _ = sys::pw_main_loop_quit(loop_ptr);
            }
        }

        let _ = thread.join();
        Err("timed out waiting for the PipeWire native DSP filter".into())
    }

    fn update(&self, state: &EqState) {
        self.shared.update(state);
    }
}

impl Drop for NativeFilterHandle {
    fn drop(&mut self) {
        let loop_ptr = self.shared.loop_ptr.load(Ordering::Acquire);
        if !loop_ptr.is_null() {
            unsafe {
                let _ = sys::pw_main_loop_quit(loop_ptr);
            }
        }

        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct FilterContext {
    shared: Arc<FilterShared>,
    in_left: *mut c_void,
    in_right: *mut c_void,
    out_left: *mut c_void,
    out_right: *mut c_void,
    left_state: [BiquadState; 31],
    right_state: [BiquadState; 31],
}

unsafe impl Send for FilterContext {}

unsafe extern "C" fn on_process(
    userdata: *mut c_void,
    position: *mut pipewire::spa::sys::spa_io_position,
) {
    let context = unsafe { &mut *(userdata as *mut FilterContext) };

    if position.is_null() {
        return;
    }

    let n_samples = unsafe { (*position).clock.duration };
    if n_samples == 0 {
        return;
    }

    let input_left = unsafe { sys::pw_filter_get_dsp_buffer(context.in_left, n_samples) }
        as *mut f32;
    let input_right = unsafe { sys::pw_filter_get_dsp_buffer(context.in_right, n_samples) }
        as *mut f32;
    let output_left = unsafe { sys::pw_filter_get_dsp_buffer(context.out_left, n_samples) }
        as *mut f32;
    let output_right = unsafe { sys::pw_filter_get_dsp_buffer(context.out_right, n_samples) }
        as *mut f32;

    if input_left.is_null()
        || input_right.is_null()
        || output_left.is_null()
        || output_right.is_null()
    {
        return;
    }

    let enabled = context.shared.enabled.load(Ordering::Relaxed);
    let preamp = f32::from_bits(context.shared.preamp.load(Ordering::Relaxed));

    let mut coefficients = [identity_coefficients(); 31];
    for (index, destination) in coefficients.iter_mut().enumerate() {
        *destination = context.shared.bands[index].load();
    }

    for sample in 0..n_samples as usize {
        let mut left = unsafe { *input_left.add(sample) } * preamp;
        let mut right = unsafe { *input_right.add(sample) } * preamp;

        if enabled {
            for index in 0..31 {
                left = context.left_state[index].process(left, coefficients[index]);
                right = context.right_state[index].process(right, coefficients[index]);
            }
        }

        unsafe {
            *output_left.add(sample) = left;
            *output_right.add(sample) = right;
        }
    }
}

fn run_filter(shared: Arc<FilterShared>, target: String) {
    unsafe {
        sys::pw_init(ptr::null_mut(), ptr::null_mut());
    }

    let loop_ptr = unsafe { sys::pw_main_loop_new(ptr::null()) };
    if loop_ptr.is_null() {
        shared.failed.store(true, Ordering::Release);
        unsafe {
            sys::pw_deinit();
        }
        return;
    }

    shared.loop_ptr.store(loop_ptr, Ordering::Release);

    let props = match properties_for_filter(&target) {
        Ok(props) => props,
        Err(_) => {
            shared.failed.store(true, Ordering::Release);
            unsafe {
                sys::pw_main_loop_destroy(loop_ptr);
                sys::pw_deinit();
            }
            return;
        }
    };

    let mut events: sys::pw_filter_events = unsafe { std::mem::zeroed() };
    events.version = 1;
    events.process = Some(on_process);

    let mut context = Box::new(FilterContext {
        shared: Arc::clone(&shared),
        in_left: ptr::null_mut(),
        in_right: ptr::null_mut(),
        out_left: ptr::null_mut(),
        out_right: ptr::null_mut(),
        left_state: [BiquadState::default(); 31],
        right_state: [BiquadState::default(); 31],
    });

    let loop_impl = sys::pw_main_loop_get_loop(loop_ptr);
    let name = CString::new(FILTER_NODE).expect("static node name has no NUL");

    let filter = unsafe {
        sys::pw_filter_new_simple(
            loop_impl,
            name.as_ptr(),
            props,
            &events,
            context.as_mut() as *mut FilterContext as *mut c_void,
        )
    };

    if filter.is_null() {
        shared.failed.store(true, Ordering::Release);
        unsafe {
            sys::pw_main_loop_destroy(loop_ptr);
            sys::pw_deinit();
        }
        return;
    }


    context.in_left = add_port(filter, sys::PW_DIRECTION_INPUT, "input_FL", "FL");
    context.in_right = add_port(filter, sys::PW_DIRECTION_INPUT, "input_FR", "FR");
    context.out_left = add_port(filter, sys::PW_DIRECTION_OUTPUT, "output_FL", "FL");
    context.out_right = add_port(filter, sys::PW_DIRECTION_OUTPUT, "output_FR", "FR");

    if context.in_left.is_null()
        || context.in_right.is_null()
        || context.out_left.is_null()
        || context.out_right.is_null()
    {
        shared.failed.store(true, Ordering::Release);
        unsafe {
            sys::pw_filter_destroy(filter);
            sys::pw_main_loop_destroy(loop_ptr);
            sys::pw_deinit();
        }
        return;
    }

    let result = unsafe {
        sys::pw_filter_connect(
            filter,
            sys::pw_filter_flags_PW_FILTER_FLAG_RT_PROCESS as _,
            ptr::null(),
            0,
        )
    };

    if result < 0 {
        shared.failed.store(true, Ordering::Release);
        unsafe {
            sys::pw_filter_destroy(filter);
            sys::pw_main_loop_destroy(loop_ptr);
            sys::pw_deinit();
        }
        return;
    }

    shared.ready.store(true, Ordering::Release);

    unsafe {
        let _ = sys::pw_main_loop_run(loop_ptr);
        sys::pw_filter_destroy(filter);
        sys::pw_main_loop_destroy(loop_ptr);
        sys::pw_deinit();
    }
}

fn properties_for_filter(target: &str) -> Result<*mut sys::pw_properties, String> {
    let target = target.replace('\\', "\\\\").replace(' ', "\\ ");
    let description = format!(
        "media.type=Audio media.category=Sink media.role=DSP media.class=Audio/Sink          node.name={FILTER_SINK} node.description=\"{FILTER_NODE}\"          node.virtual=true node.autoconnect=true target.object={target}          audio.rate=48000"
    );

    let value = CString::new(description)
        .map_err(|_| "invalid PipeWire filter properties".to_string())?;

    let props = unsafe { sys::pw_properties_new_string(value.as_ptr()) };
    if props.is_null() {
        Err("failed to allocate PipeWire filter properties".into())
    } else {
        Ok(props)
    }
}

#[repr(C)]
struct PortMarker(u8);

fn add_port(
    filter: *mut sys::pw_filter,
    direction: sys::pw_direction,
    name: &str,
    position: &str,
) -> *mut c_void {
    let properties = format!(
        "format.dsp=32\\ bit\\ float\\ mono\\ audio port.name={name} audio.channel={position}"
    );
    let properties = match CString::new(properties) {
        Ok(value) => value,
        Err(_) => return ptr::null_mut(),
    };

    let props = unsafe { sys::pw_properties_new_string(properties.as_ptr()) };
    if props.is_null() {
        return ptr::null_mut();
    }

    unsafe {
        sys::pw_filter_add_port(
            filter,
            direction,
            sys::pw_filter_port_flags_PW_FILTER_PORT_FLAG_MAP_BUFFERS as _,
            std::mem::size_of::<PortMarker>(),
            props,
            ptr::null(),
            0,
        )
    }
}

fn identity_coefficients() -> Coefficients {
    Coefficients {
        b0: 1.0,
        ..Coefficients::default()
    }
}

fn db_to_linear(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

fn peaking_coefficients(frequency: f32, gain_db: f32, sample_rate: f32, q: f32) -> Coefficients {
    if gain_db.abs() < f32::EPSILON || frequency <= 0.0 || frequency >= sample_rate * 0.5 {
        return Coefficients {
            b0: 1.0,
            ..Coefficients::default()
        };
    }

    let a = 10.0f32.powf(gain_db / 40.0);
    let omega = 2.0 * std::f32::consts::PI * frequency / sample_rate;
    let sin = omega.sin();
    let cos = omega.cos();
    let alpha = sin / (2.0 * q);

    let b0 = 1.0 + alpha * a;
    let b1 = -2.0 * cos;
    let b2 = 1.0 - alpha * a;
    let a0 = 1.0 + alpha / a;
    let a1 = -2.0 * cos;
    let a2 = 1.0 - alpha / a;

    Coefficients {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
    }
}

pub struct DspController {
    tx: std::sync::mpsc::Sender<EqState>,
}

impl DspController {
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<EqState>();

        thread::spawn(move || {
            let mut runtime = DspRuntime::new();

            while let Ok(mut state) = rx.recv() {
                while let Ok(next) = rx.recv_timeout(std::time::Duration::from_millis(120)) {
                    state = next;
                }

                if let Err(error) = runtime.apply(&state) {
                    eprintln!("Harmonic DSP: {error}");
                }
            }

            runtime.stop();
        });

        Self { tx }
    }

    pub fn apply(&self, state: &EqState) {
        let _ = self.tx.send(state.clone());
    }
}

struct DspRuntime {
    filter: Option<NativeFilterHandle>,
    original_default_sink: Option<String>,
}

impl DspRuntime {
    fn new() -> Self {
        Self {
            filter: None,
            original_default_sink: None,
        }
    }

    fn apply(&mut self, state: &EqState) -> Result<(), String> {
        if !state.enabled {
            if let Some(filter) = &self.filter {
                filter.update(state);
            }

            self.restore_default();
            return Ok(());
        }

        let target = self
            .original_default_sink
            .clone()
            .or_else(default_sink)
            .ok_or_else(|| "could not determine the current PipeWire default sink".to_string())?;

        if self.original_default_sink.is_none() {
            self.original_default_sink = Some(target.clone());
        }

        if let Some(filter) = &self.filter {
            filter.update(state);
        } else {
            self.filter = Some(NativeFilterHandle::start(state, &target)?);
        }

        let status = std::process::Command::new("wpctl")
            .args(["set-default", FILTER_SINK])
            .status()
            .map_err(|e| format!("failed to select Harmonic output: {e}"))?;

        if !status.success() {
            return Err("wpctl could not select the Harmonic EQ sink".into());
        }

        Ok(())
    }

    fn restore_default(&mut self) {
        if let Some(original) = &self.original_default_sink {
            let _ = std::process::Command::new("wpctl")
                .args(["set-default", original])
                .status();
        }
    }

    fn stop(&mut self) {
        self.restore_default();
        self.filter.take();
        self.original_default_sink.take();
    }
}

impl Drop for DspRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}

fn default_sink() -> Option<String> {
    let output = std::process::Command::new("wpctl")
        .arg("get-default")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_coefficients_are_identity() {
        let c = peaking_coefficients(1_000.0, 0.0, SAMPLE_RATE, Q);
        assert!((c.b0 - 1.0).abs() < f32::EPSILON);
        assert!(c.b1.abs() < f32::EPSILON);
        assert!(c.b2.abs() < f32::EPSILON);
        assert!(c.a1.abs() < f32::EPSILON);
        assert!(c.a2.abs() < f32::EPSILON);
    }

    #[test]
    fn positive_gain_increases_center_frequency_response() {
        let c = peaking_coefficients(1_000.0, 6.0, SAMPLE_RATE, Q);
        let w = 2.0 * std::f32::consts::PI * 1_000.0 / SAMPLE_RATE;
        let real_num = c.b0 + c.b1 * w.cos() + c.b2 * (2.0 * w).cos();
        let imag_num = -c.b1 * w.sin() - c.b2 * (2.0 * w).sin();
        let real_den = 1.0 + c.a1 * w.cos() + c.a2 * (2.0 * w).cos();
        let imag_den = -c.a1 * w.sin() - c.a2 * (2.0 * w).sin();
        let magnitude = real_num.hypot(imag_num) / real_den.hypot(imag_den);
        assert!(magnitude > 1.9);
    }

    #[test]
    fn all_band_frequencies_are_representable_at_48khz() {
        assert!(BAND_FREQUENCIES.iter().all(|frequency| *frequency < SAMPLE_RATE * 0.5));
    }

    #[test]
    fn preamp_conversion_is_unity_at_zero_db() {
        assert!((db_to_linear(0.0) - 1.0).abs() < f32::EPSILON);
    }
}
