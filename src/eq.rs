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

    pub fn filter_chain_config(&self) -> String {
        let mut config = String::from("filter.graph = {\n    nodes = [\n");
        for (index, band) in self.bands.iter().enumerate() {
            if !band.enabled || !self.enabled { continue; }
            let _ = writeln!(config, "        {{ type = builtin name = eq{index} label = bq_peaking control = {{ Freq = {} Gain = {} Q = 1.000 }} }},", band.frequency, band.gain_db);
        }
        config.push_str("    ]\n}\n");
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
}
