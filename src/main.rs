mod dsp;
mod eq;
mod pipewire;

use gtk4 as gtk;
use gtk::prelude::*;
use gtk::{
    glib, Align, Application, ApplicationWindow, Box as GtkBox, Button, DrawingArea, Label,
    Orientation, Scale, Separator,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

const APP_ID: &str = "io.github.GenXinvenits.Harmonic";

fn main() -> glib::ExitCode {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &Application) {
    let (tx, rx) = mpsc::channel();
    pipewire::spawn_discovery(tx);

    let eq_state = Rc::new(RefCell::new(eq::EqState::default()));
    let dsp = Rc::new(dsp::DspController::new());

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Harmonic")
        .default_width(1180)
        .default_height(760)
        .build();

    let root = GtkBox::new(Orientation::Vertical, 0);
    root.set_margin_top(18);
    root.set_margin_bottom(18);
    root.set_margin_start(22);
    root.set_margin_end(22);

    let header = GtkBox::new(Orientation::Horizontal, 12);
    header.set_valign(Align::Center);

    let title = Label::new(Some("Harmonic"));
    title.add_css_class("title-1");
    title.set_halign(Align::Start);

    let subtitle = Label::new(Some("Native audio DSP for Linux"));
    subtitle.add_css_class("dim-label");
    subtitle.set_halign(Align::Start);

    let title_box = GtkBox::new(Orientation::Vertical, 2);
    title_box.append(&title);
    title_box.append(&subtitle);

    let status = Label::new(Some("Connecting to PipeWire…"));
    status.add_css_class("dim-label");
    status.set_hexpand(true);
    status.set_halign(Align::End);

    let power = Button::with_label("Enabled");
    power.add_css_class("suggested-action");
    {
        let state = eq_state.clone();
        let dsp = dsp.clone();
        let power_label = power.clone();

        power.connect_clicked(move |_| {
            let mut state = state.borrow_mut();
            state.enabled = !state.enabled;

            power_label.set_label(if state.enabled { "Enabled" } else { "Disabled" });
            if state.enabled {
                power_label.add_css_class("suggested-action");
            } else {
                power_label.remove_css_class("suggested-action");
            }

            dsp.apply(&state);
        });
    }

    header.append(&title_box);
    header.append(&status);
    header.append(&power);
    root.append(&header);
    root.append(&Separator::new(Orientation::Horizontal));

    let controls = GtkBox::new(Orientation::Horizontal, 12);
    controls.set_margin_top(18);

    let output = GtkBox::new(Orientation::Vertical, 6);
    let output_label = Label::new(Some("Output"));
    output_label.add_css_class("heading");
    let output_value = Label::new(Some("Harmonic EQ → default sink"));
    output_value.set_halign(Align::Start);
    output.append(&output_label);
    output.append(&output_value);

    let preset = GtkBox::new(Orientation::Vertical, 6);
    let preset_label = Label::new(Some("Preset"));
    preset_label.add_css_class("heading");
    let preset_value = Label::new(Some("Flat"));
    preset_value.set_halign(Align::Start);
    preset.append(&preset_label);
    preset.append(&preset_value);

    let preamp = GtkBox::new(Orientation::Vertical, 6);
    let preamp_label = Label::new(Some("Preamp"));
    preamp_label.add_css_class("heading");
    let preamp_value = Label::new(Some("0.0 dB"));
    preamp_value.set_halign(Align::Start);

    let preamp_slider = Scale::with_range(Orientation::Horizontal, -12.0, 12.0, 0.5);
    preamp_slider.set_value(0.0);
    preamp_slider.set_draw_value(false);

    {
        let state = eq_state.clone();
        let dsp = dsp.clone();
        let value = preamp_value.clone();

        preamp_slider.connect_value_changed(move |scale| {
            let gain = scale.value() as f32;
            let mut state = state.borrow_mut();
            state.preamp_db = gain;
            value.set_text(&format!("{gain:.1} dB"));
            dsp.apply(&state);
        });
    }

    preamp.append(&preamp_label);
    preamp.append(&preamp_value);
    preamp.append(&preamp_slider);

    controls.append(&output);
    controls.append(&Separator::new(Orientation::Vertical));
    controls.append(&preset);
    controls.append(&Separator::new(Orientation::Vertical));
    controls.append(&preamp);
    root.append(&controls);

    let eq_title = Label::new(Some("31-Band Graphic Equalizer"));
    eq_title.add_css_class("title-2");
    eq_title.set_halign(Align::Start);
    eq_title.set_margin_top(28);
    root.append(&eq_title);

    let graph = DrawingArea::new();
    graph.set_content_width(800);
    graph.set_content_height(190);
    graph.set_hexpand(true);
    graph.set_margin_top(10);
    graph.add_css_class("response-graph");

    {
        let state = eq_state.clone();
        graph.set_draw_func(move |_, cr, width, height| {
            draw_response(cr, width as f64, height as f64, &state.borrow());
        });
    }
    root.append(&graph);

    let surface = GtkBox::new(Orientation::Vertical, 0);
    surface.set_vexpand(true);
    surface.set_margin_top(8);
    surface.set_margin_bottom(8);
    surface.add_css_class("eq-surface");

    let sliders = GtkBox::new(Orientation::Horizontal, 3);
    sliders.set_homogeneous(true);
    sliders.set_valign(Align::Fill);
    sliders.set_margin_start(8);
    sliders.set_margin_end(8);

    for (index, band) in eq::BAND_FREQUENCIES.iter().enumerate() {
        let column = GtkBox::new(Orientation::Vertical, 4);
        column.set_valign(Align::Fill);

        let value = Label::new(Some("0.0"));
        value.add_css_class("dim-label");

        let slider = Scale::with_range(Orientation::Vertical, -12.0, 12.0, 0.5);
        slider.set_value(0.0);
        slider.set_vexpand(true);
        slider.set_draw_value(false);
        slider.set_inverted(true);
        slider.set_tooltip_text(Some(&format!("{band:.1} Hz")));

        let state = eq_state.clone();
        let dsp = dsp.clone();
        let value_clone = value.clone();
        let graph_clone = graph.clone();

        slider.connect_value_changed(move |scale| {
            let gain = scale.value() as f32;
            let mut state = state.borrow_mut();
            state.set_band_gain(index, gain);
            value_clone.set_text(&format!("{gain:.1}"));
            graph_clone.queue_draw();
            dsp.apply(&state);
        });

        let frequency = Label::new(Some(&format_frequency(*band)));
        frequency.add_css_class("caption");
        frequency.set_halign(Align::Center);

        column.append(&value);
        column.append(&slider);
        column.append(&frequency);
        sliders.append(&column);
    }

    surface.append(&sliders);
    root.append(&surface);

    let footer = GtkBox::new(Orientation::Horizontal, 8);
    footer.set_margin_top(10);
    let info = Label::new(Some("PipeWire filter-chain • 31-band EQ • live DSP engine"));
    info.add_css_class("dim-label");
    info.set_halign(Align::Start);
    footer.append(&info);
    root.append(&footer);

    let css = gtk::CssProvider::new();
    css.load_from_data(
        ".eq-surface { padding: 10px; border-radius: 14px; background: alpha(@theme_fg_color, 0.04); }\n         .response-graph { border-radius: 12px; background: alpha(@theme_fg_color, 0.025); }\n         .heading { font-weight: 600; }\n         scale trough { min-width: 7px; }\n         scale slider { min-width: 16px; min-height: 16px; }",
    );

    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("GTK display unavailable"),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let status_for_poll = status.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        if let Some(snapshot) = rx.try_iter().last() {
            if snapshot.connected {
                status_for_poll.set_text(&format!(
                    "PipeWire connected • {} nodes • DSP ready",
                    snapshot.node_count
                ));
            } else {
                status_for_poll.set_text("PipeWire unavailable");
            }
        }
        glib::ControlFlow::Continue
    });

    dsp.apply(&eq_state.borrow());

    window.set_child(Some(&root));
    window.present();
}

fn draw_response(
    cr: &gtk::cairo::Context,
    width: f64,
    height: f64,
    state: &eq::EqState,
) {
    if width <= 20.0 || height <= 20.0 {
        return;
    }

    let left = 42.0;
    let right = width - 16.0;
    let top = 14.0;
    let bottom = height - 26.0;

    cr.set_line_width(1.0);

    for db in [-18.0, -12.0, -6.0, 0.0, 6.0, 12.0, 18.0] {
        let y = map_db(db, top, bottom);
        cr.move_to(left, y);
        cr.line_to(right, y);
        let _ = cr.stroke();
    }

    for frequency in [20.0, 100.0, 1_000.0, 10_000.0, 20_000.0] {
        let x = map_frequency(frequency, left, right);
        cr.move_to(x, top);
        cr.line_to(x, bottom);
        let _ = cr.stroke();
    }

    cr.set_line_width(2.0);
    let samples = 240usize;
    for i in 0..=samples {
        let t = i as f32 / samples as f32;
        let frequency = 20.0 * (1_000.0_f32).powf(t);
        let x = map_frequency(frequency, left, right);
        let y = map_db(state.response_db(frequency), top, bottom);

        if i == 0 {
            cr.move_to(x, y);
        } else {
            cr.line_to(x, y);
        }
    }
    let _ = cr.stroke();
}

fn map_frequency(frequency: f32, left: f64, right: f64) -> f64 {
    let min = 20.0_f32.ln();
    let max = 20_000.0_f32.ln();
    let value = frequency.max(20.0).min(20_000.0).ln();
    left + ((value - min) / (max - min)) as f64 * (right - left)
}

fn map_db(db: f32, top: f64, bottom: f64) -> f64 {
    let normalized = ((db.clamp(-18.0, 18.0) + 18.0) / 36.0) as f64;
    bottom - normalized * (bottom - top)
}

fn format_frequency(frequency: f32) -> String {
    match frequency as u32 {
        1_000 => "1k".into(),
        1_250 => "1.25k".into(),
        1_600 => "1.6k".into(),
        2_000 => "2k".into(),
        2_500 => "2.5k".into(),
        3_150 => "3.15k".into(),
        4_000 => "4k".into(),
        5_000 => "5k".into(),
        6_300 => "6.3k".into(),
        8_000 => "8k".into(),
        10_000 => "10k".into(),
        12_500 => "12.5k".into(),
        16_000 => "16k".into(),
        20_000 => "20k".into(),
        value => value.to_string(),
    }
}
