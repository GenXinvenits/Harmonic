mod eq;
mod pipewire;

use gtk4 as gtk;
use gtk::prelude::*;
use gtk::{glib, Align, Application, ApplicationWindow, Box as GtkBox, Button, Label, Orientation, Scale, Separator};
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

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Harmonic")
        .default_width(1120)
        .default_height(720)
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
        let power_label = power.clone();
        power.connect_clicked(move |_| {
            let mut state = state.borrow_mut();
            state.enabled = !state.enabled;
            power_label.set_label(if state.enabled { "Enabled" } else { "Disabled" });
            if state.enabled { power_label.add_css_class("suggested-action"); }
            else { power_label.remove_css_class("suggested-action"); }
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
    let output_value = Label::new(Some("Default PipeWire sink"));
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
        let value = preamp_value.clone();
        preamp_slider.connect_value_changed(move |scale| {
            let gain = scale.value() as f32;
            state.borrow_mut().preamp_db = gain;
            value.set_text(&format!("{gain:.1} dB"));
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

    let eq_title = Label::new(Some("Graphic Equalizer"));
    eq_title.add_css_class("title-2");
    eq_title.set_halign(Align::Start);
    eq_title.set_margin_top(28);
    root.append(&eq_title);

    let graph = GtkBox::new(Orientation::Vertical, 0);
    graph.set_vexpand(true);
    graph.set_margin_top(12);
    graph.set_margin_bottom(8);
    graph.add_css_class("eq-surface");

    let sliders = GtkBox::new(Orientation::Horizontal, 14);
    sliders.set_homogeneous(true);
    sliders.set_valign(Align::Center);
    sliders.set_margin_start(20);
    sliders.set_margin_end(20);

    for (index, band) in eq::BAND_FREQUENCIES.iter().enumerate() {
        let column = GtkBox::new(Orientation::Vertical, 8);
        column.set_valign(Align::Fill);

        let value = Label::new(Some("0.0"));
        value.add_css_class("dim-label");

        let slider = Scale::with_range(Orientation::Vertical, -12.0, 12.0, 0.5);
        slider.set_value(0.0);
        slider.set_vexpand(true);
        slider.set_draw_value(false);
        slider.set_inverted(true);
        slider.set_tooltip_text(Some(&format!("{band:.0} Hz")));

        let state = eq_state.clone();
        let value_clone = value.clone();
        slider.connect_value_changed(move |scale| {
            let gain = scale.value() as f32;
            state.borrow_mut().set_band_gain(index, gain);
            value_clone.set_text(&format!("{gain:.1}"));
        });

        let frequency = Label::new(Some(&format_frequency(*band)));
        frequency.add_css_class("caption");
        column.append(&value);
        column.append(&slider);
        column.append(&frequency);
        sliders.append(&column);
    }

    graph.append(&sliders);
    root.append(&graph);

    let footer = GtkBox::new(Orientation::Horizontal, 8);
    footer.set_margin_top(10);
    let info = Label::new(Some("PipeWire backend • EQ model connected • live DSP controller next"));
    info.add_css_class("dim-label");
    info.set_halign(Align::Start);
    footer.append(&info);
    root.append(&footer);

    let css = gtk::CssProvider::new();
    css.load_from_data(
        ".eq-surface { padding: 12px; border-radius: 14px; background: alpha(@theme_fg_color, 0.04); }\n\
         .heading { font-weight: 600; }\n\
         scale trough { min-width: 8px; }\n\
         scale slider { min-width: 18px; min-height: 18px; }",
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
                status_for_poll.set_text(&format!("PipeWire connected • {} nodes", snapshot.node_count));
            } else {
                status_for_poll.set_text("PipeWire unavailable");
            }
        }
        glib::ControlFlow::Continue
    });

    window.set_child(Some(&root));
    window.present();
}

fn format_frequency(frequency: f32) -> String {
    match frequency as u32 {
        1_000 => "1k".into(),
        2_000 => "2k".into(),
        4_000 => "4k".into(),
        8_000 => "8k".into(),
        16_000 => "16k".into(),
        value => value.to_string(),
    }
}
