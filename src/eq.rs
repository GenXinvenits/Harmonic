use std::f32::consts::LN_10;
use std::fmt::Write;

pub const BAND_FREQUENCIES: [f32; 31] = [
    20.0, 25.0, 31.5, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0,
    315.0, 400.0, 500.0, 630.0, 800.0, 1_000.0, 1_250.0, 1_600.0, 2_000.0,
    2_500.0, 3_150.0, 4_000.0, 5_000.0, 6_300.0, 8_000.0, 10_000.0, 12_500.0,
    16_000.0, 20_000.0,
];

#[derive(Clone, Debug)]
pub struct EqBand {
    pub frequency: f32,
    pub gain_db: f32,
    pub enabled: bool,
}

impl EqBand {
    pub fn new(frequency: f32) -> Self {
        Self {
            frequency,
            gain_db: 0.0,
            enabled: true,
        }
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
        let mut config = String::from(
            "filter.graph = {\n    nodes = [\n        {\n            type = builtin\n            name = harmonic_eq\n            label = param_eq\n            config = {\n                filters = [\n",
        );

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

    /// Approximate the graphic EQ response for the UI.
    /// This mirrors the broad shape of each peaking filter without pretending
    /// to be the exact PipeWire biquad response.
    pub fn response_db(&self, frequency: f32) -> f32 {
        let mut response = self.preamp_db;

        if self.enabled && frequency > 0.0 {
            for band in &self.bands {
                if !band.enabled || band.gain_db.abs() < f32::EPSILON {
                    continue;
                }

                let ratio = (frequency / band.frequency).ln() / LN_10;
                let width = 0.22_f32;
                let shape = (-(ratio / width).powi(2)).exp();
                response += band.gain_db * shape;
            }
        }

        response.clamp(-18.0, 18.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_state_has_thirty_one_bands() {
        assert_eq!(EqState::default().bands.len(), 31);
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
        eq.set_band_gain(14, 3.0);
        let config = eq.filter_chain_config();
        assert!(config.contains("label = param_eq"));
        assert!(config.contains("freq = 500.0 gain = 3.00"));
    }

    #[test]
    fn response_is_flat_by_default() {
        let eq = EqState::default();
        assert!(eq.response_db(1_000.0).abs() < f32::EPSILON);
    }

    #[test]
    fn response_tracks_band_gain() {
        let mut eq = EqState::default();
        eq.set_band_gain(14, 6.0);
        assert!(eq.response_db(500.0) > 5.0);
    }
}
