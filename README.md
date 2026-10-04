# Harmonic

**Native audio DSP for Linux.**

Harmonic is a lightweight GTK4 application for controlling PipeWire-based audio processing.

## Current status

The `foundation` branch now contains the first usable EQ surface and live DSP path:

- Rust + GTK4 application
- Native PipeWire connection and registry discovery
- Live PipeWire node count
- Shared EQ state model
- **31-band graphic EQ** using standard ISO-style center frequencies from 20 Hz to 20 kHz
- Preamp and master enable state
- PipeWire builtin parametric EQ graph generation
- Live filter-chain runtime controller
- Harmonic virtual EQ sink routed to the sink active when Harmonic was enabled
- Automatic default-sink switching and restoration
- **Live response graph** with logarithmic frequency scale
- CI for formatting, tests, and Clippy

### Live DSP architecture

Harmonic launches a dedicated PipeWire filter-chain client for its DSP graph. The filter-chain exposes an `Audio/Sink` node named `harmonic_eq`; its playback side targets the physical sink that was active when Harmonic was enabled. Harmonic then makes `harmonic_eq` the default output through `wpctl`.

EQ changes are coalesced for 120 ms before the filter-chain is rebuilt. This remains the current runtime limitation; the next DSP iteration will keep one graph alive and update filter parameters in-place.

## Roadmap

1. Foundation — GTK4 application and PipeWire discovery
2. DSP runtime — live filter-chain client and routing
3. **Live parameter control** — update EQ coefficients without rebuilding the graph
4. **Equalizer** — 31-band graphic EQ, preamp, and response graph
5. Output — physical output selector and output-aware routing
6. Presets — built-in and user presets with per-output profiles
7. Advanced DSP — compressor, limiter, bass enhancement, loudness, and crossfeed
8. Packaging — native Linux packages and Flatpak

## Building

Install the native development dependencies, then:

```bash
cargo run
```

For Debian/Ubuntu-based systems:

```bash
sudo apt install build-essential pkg-config libgtk-4-dev libpipewire-0.3-dev pipewire-bin
```

The application currently targets Linux with PipeWire and GTK4.

## License

GPL-3.0-or-later
