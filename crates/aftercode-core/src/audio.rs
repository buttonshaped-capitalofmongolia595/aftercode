use serde::{Deserialize, Serialize};

/// Default sample rate (ElevenLabs pcm_44100, mock).
pub const SAMPLE_RATE: u32 = 44_100;
/// OpenAI `/v1/audio/speech` PCM output rate.
pub const OPENAI_SAMPLE_RATE: u32 = 24_000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum VoiceRole {
    Host,
    Expert,
}

/// Mono i16 PCM. The sample rate is carried alongside the buffer by the caller
/// (each TTS provider reports its own rate via `TtsProvider::sample_rate`).
#[derive(Debug, Clone, PartialEq)]
pub struct PcmAudio {
    pub samples: Vec<i16>,
}

impl PcmAudio {
    /// `ms` of silence at the given sample rate.
    pub fn silence(ms: u32, sample_rate: u32) -> Self {
        let n = (sample_rate as u64 * ms as u64 / 1000) as usize;
        PcmAudio {
            samples: vec![0i16; n],
        }
    }
    pub fn duration_seconds(&self, sample_rate: u32) -> f32 {
        self.samples.len() as f32 / sample_rate as f32
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
        assert_eq!(
            PcmAudio::silence(1000, SAMPLE_RATE).samples.len(),
            SAMPLE_RATE as usize
        );
        assert_eq!(
            PcmAudio::silence(500, SAMPLE_RATE).samples.len(),
            (SAMPLE_RATE / 2) as usize
        );
    }
    #[test]
    fn silence_respects_custom_rate() {
        assert_eq!(
            PcmAudio::silence(1000, OPENAI_SAMPLE_RATE).samples.len(),
            OPENAI_SAMPLE_RATE as usize
        );
    }
}
