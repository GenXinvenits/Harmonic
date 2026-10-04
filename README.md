# Harmonic

**Native audio DSP for Linux.**

Harmonic is a lightweight GTK4 application for controlling PipeWire-based audio processing. The goal is to provide a polished graphical equalizer and DSP workstation without replacing PipeWire or introducing a heavyweight audio stack.

## Current status

The `foundation` branch contains the first application shell:

- Rust application using GTK4
- Native PipeWire connection and registry discovery
- Live PipeWire node count in the UI
- 10-band graphic EQ interface prototype
- Preamp, preset, output, and enable controls staged for the DSP layer

## Roadmap

1. **Foundation** — GTK4 application and PipeWire discovery
2. **DSP engine** — PipeWire filter-chain graph and live parameter control
3. **Equalizer** — 10/15/31-band graphic EQ, preamp, and response graph
4. **Presets** — built-in and user presets with per-output profiles
5. **Advanced DSP** — parametric EQ, compressor, limiter, bass enhancement, loudness, and crossfeed
6. **Packaging** — native Linux packages and Flatpak

## Building

Install the native development dependencies for GTK4 and PipeWire, then:

```bash
cargo run
```

The application currently targets Linux with PipeWire and GTK4.

## License

GPL-3.0-or-later
