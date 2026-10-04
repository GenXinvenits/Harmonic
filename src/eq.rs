use std::fmt::Write;

pub const BAND_FREQUENCIES: [f32; 10] = [31.0, 62.0, 125.0, 250.0, 500.0, 1_000.0, 2_000.0, 4_000.0, 8_000.0, 16_000.0];

#[derive(Clone, Debug)]
pub struct EqBand {
    pub frequency: f32,
    pub gain_db: f32,
    pub enabled: bool,
}

impl EqBand {
    pub fn new(frequency: f32) -> Self {
        Self { frequency, gain_db: 0.0, enabled: true }
    }
}

#[derive(Clone, Debug)]
pub struct EqState {
    pub preamp_db: f32,
    pub enabled: bool,
    pub bands: Vec<EqBand>,
}

impl Default for EqState {
    fn default() -> Self {
        Self {
            preamp_db: 0.0,
            enabled: true,
            bands: BAND_FREQUENCIES.into_iter().map(EqBand::new).collect(),
        }
    }
}

impl EqState {
    pub fn set_band_gain(&mut self, index: usize, gain_db: f32) {
        if let Some(band) = self.bands.get_mut(index) {
            band.gain_db = gain_db.clamp(-12.0, 12.0);
        }
    }

    /// Generate a PipeWire builtin parametric-EQ graph from the current model.
    pub fn filter_chain_config(&self) -> String {
        let mut config = String::from("filter.graph = {\n    nodes = [\n        {\n            type = builtin\n            name = harmonic_eq\n            label = param_eq\n            config = {\n                filters = [\n");

        if self.enabled {
            for band in &self.bands {
                if band.enabled && band.gain_db.abs() > f32::EPSILON {
                    let _ = writeln!(
                        config,
                        "                    {{ type = bq_peaking freq = {:.1} gain = {:.2} q = 1.000 }},",
                        band.frequency, band.gain_db
                    );
                }
            }
        }

        config.push_str("                ]\n            }\n        }\n    ]\n}\n");
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_state_has_ten_bands() {
        assert_eq!(EqState::default().bands.len(), 10);
    }

    #[test]
    fn gain_is_clamped() {
        let mut eq = EqState::default();
        eq.set_band_gain(0, 50.0);
        assert_eq!(eq.bands[0].gain_db, 12.0);
    }

    #[test]
    fn config_contains_parametric_eq() {
        let mut eq = EqState::default();
        eq.set_band_gain(0, 3.0);
        let config = eq.filter_chain_config();
        assert!(config.contains("label = param_eq"));
        assert!(config.contains("freq = 31.0 gain = 3.00"));
    }
}
