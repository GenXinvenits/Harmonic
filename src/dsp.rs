use crate::eq::EqState;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const FILTER_NODE: &str = "Harmonic EQ";
const FILTER_SINK: &str = "harmonic_eq";

pub struct DspController {
    tx: Sender<EqState>,
}

impl DspController {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<EqState>();

        thread::spawn(move || {
            let mut runtime = DspRuntime::new();

            while let Ok(mut state) = rx.recv() {
                while let Ok(next) = rx.recv_timeout(Duration::from_millis(120)) {
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
    child: Option<Child>,
    original_default_sink: Option<String>,
}

impl DspRuntime {
    fn new() -> Self {
        Self {
            child: None,
            original_default_sink: None,
        }
    }

    fn apply(&mut self, state: &EqState) -> Result<(), String> {
        if !state.enabled {
            self.stop();
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

        self.stop_filter_only();

        let config = render_filter_config(state, &target);
        let path = write_runtime_config(&config)?;

        let child = Command::new("pipewire")
            .arg("-c")
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("failed to start PipeWire filter-chain: {e}"))?;

        self.child = Some(child);

        thread::sleep(Duration::from_millis(180));

        let status = Command::new("wpctl")
            .args(["set-default", FILTER_SINK])
            .status()
            .map_err(|e| format!("failed to select Harmonic output: {e}"))?;

        if !status.success() {
            self.stop_filter_only();
            return Err("wpctl could not select the Harmonic EQ sink".into());
        }

        let _ = std::fs::remove_file(path);
        Ok(())
    }

    fn stop_filter_only(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn stop(&mut self) {
        self.stop_filter_only();

        if let Some(original) = self.original_default_sink.take() {
            let _ = Command::new("wpctl")
                .args(["set-default", &original])
                .status();
        }
    }
}

impl Drop for DspRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}

fn default_sink() -> Option<String> {
    let output = Command::new("wpctl").arg("get-default").output().ok()?;
    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn write_runtime_config(config: &str) -> Result<std::path::PathBuf, String> {
    let mut path = std::env::temp_dir();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    path.push(format!("harmonic-filter-{nonce}.conf"));

    std::fs::write(&path, config)
        .map_err(|e| format!("failed to write PipeWire filter configuration: {e}"))?;

    Ok(path)
}

fn render_filter_config(state: &EqState, target: &str) -> String {
    let mut config = state.filter_chain_config();

    let preamp = state.preamp_db;

    if preamp.abs() > f32::EPSILON {
        let marker = "filters = [";
        if let Some(pos) = config.find(marker) {
            let insert_at = pos + marker.len();
            config.insert_str(
                insert_at,
                &format!(
                    "\n                    {{ type = bq_highshelf freq = 0.0 gain = {:.2} q = 1.000 }},",
                    preamp
                ),
            );
        }
    }

    format!(
        "context.modules = [\n         {{\n             name = libpipewire-module-filter-chain\n             args = {{\n                 node.description = \"{FILTER_NODE}\"\n                 media.name = \"{FILTER_NODE}\"\n                 audio.channels = 2\n                 audio.position = [ FL FR ]\n                 {config}                 capture.props = {{\n                     node.name = \"{FILTER_SINK}\"\n                     media.class = Audio/Sink\n                     audio.channels = 2\n                     audio.position = [ FL FR ]\n                 }}\n                 playback.props = {{\n                     node.name = \"harmonic_output\"\n                     target.object = \"{target}\"\n                     node.passive = true\n                     audio.channels = 2\n                     audio.position = [ FL FR ]\n                 }}\n             }}\n         }}\n         ]\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eq::EqState;

    #[test]
    fn runtime_config_has_virtual_sink_and_target() {
        let mut state = EqState::default();
        state.set_band_gain(4, 4.0);

        let config = render_filter_config(&state, "alsa_output.test");
        assert!(config.contains("media.class = Audio/Sink"));
        assert!(config.contains("node.name = \"harmonic_eq\""));
        assert!(config.contains("target.object = \"alsa_output.test\""));
        assert!(config.contains("freq = 500.0 gain = 4.00"));
    }

    #[test]
    fn preamp_is_rendered_as_zero_frequency_high_shelf() {
        let mut state = EqState::default();
        state.preamp_db = -5.0;

        let config = render_filter_config(&state, "alsa_output.test");
        assert!(config.contains("bq_highshelf freq = 0.0 gain = -5.00"));
    }
}
