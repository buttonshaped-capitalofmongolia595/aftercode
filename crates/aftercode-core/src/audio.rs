use serde::{Deserialize, Serialize};

pub const SAMPLE_RATE: u32 = 44_100;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum VoiceRole {
    Host,
    Expert,
}

/// Mono i16 PCM at SAMPLE_RATE.
#[derive(Debug, Clone, PartialEq)]
pub struct PcmAudio {
    pub samples: Vec<i16>,
}

impl PcmAudio {
    pub fn silence(ms: u32) -> Self {
        let n = (SAMPLE_RATE as u64 * ms as u64 / 1000) as usize;
        PcmAudio {
            samples: vec![0i16; n],
        }
    }
    pub fn duration_seconds(&self) -> f32 {
        self.samples.len() as f32 / SAMPLE_RATE as f32
    }
}

/// Pause lengths in ms (PRD §14).
pub const GAP_SAME_SPEAKER_MS: u32 = 300;
pub const GAP_SPEAKER_SWITCH_MS: u32 = 600;
pub const GAP_SECTION_MS: u32 = 1000;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silence_length_matches_rate() {
        assert_eq!(PcmAudio::silence(1000).samples.len(), SAMPLE_RATE as usize);
        assert_eq!(
            PcmAudio::silence(500).samples.len(),
            (SAMPLE_RATE / 2) as usize
        );
    }
}
