use aftercode_core::audio::{PcmAudio, GAP_SAME_SPEAKER_MS, GAP_SPEAKER_SWITCH_MS};
use aftercode_core::episode::Speaker;

/// One synthesized segment plus the speaker that produced it.
pub struct RenderedSegment {
    pub speaker: Speaker,
    pub audio: PcmAudio,
}

/// Concatenate segments, inserting a silence gap before each (except the first):
/// speaker switch -> GAP_SPEAKER_SWITCH_MS, same speaker -> GAP_SAME_SPEAKER_MS.
/// `sample_rate` is the rate of every segment (one TTS provider per episode).
pub fn concat_with_gaps(segments: &[RenderedSegment], sample_rate: u32) -> PcmAudio {
    let mut out: Vec<i16> = Vec::new();
    let mut prev: Option<Speaker> = None;
    for seg in segments {
        if let Some(p) = prev {
            let gap = if p == seg.speaker {
                GAP_SAME_SPEAKER_MS
            } else {
                GAP_SPEAKER_SWITCH_MS
            };
            out.extend_from_slice(&PcmAudio::silence(gap, sample_rate).samples);
        }
        out.extend_from_slice(&seg.audio.samples);
        prev = Some(seg.speaker);
    }
    PcmAudio { samples: out }
}

/// Encode mono i16 PCM to an MP3 byte buffer at the given sample rate.
pub fn encode_mp3(pcm: &PcmAudio, sample_rate: u32) -> anyhow::Result<Vec<u8>> {
    use mp3lame_encoder::{Builder, FlushNoGap, MonoPcm};
    let mut builder = Builder::new().ok_or_else(|| anyhow::anyhow!("lame builder"))?;
    builder
        .set_num_channels(1)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    builder
        .set_sample_rate(sample_rate)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    builder
        .set_brate(mp3lame_encoder::Bitrate::Kbps128)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let mut enc = builder.build().map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let mut mp3 = Vec::with_capacity(pcm.samples.len());
    let n = enc
        .encode(MonoPcm(&pcm.samples), mp3.spare_capacity_mut())
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    unsafe {
        mp3.set_len(n);
    }
    let tail = enc
        .flush::<FlushNoGap>(mp3.spare_capacity_mut())
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    unsafe {
        mp3.set_len(mp3.len() + tail);
    }
    Ok(mp3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aftercode_core::audio::{OPENAI_SAMPLE_RATE, SAMPLE_RATE};

    fn seg(sp: Speaker, n: usize) -> RenderedSegment {
        RenderedSegment {
            speaker: sp,
            audio: PcmAudio {
                samples: vec![100i16; n],
            },
        }
    }

    #[test]
    fn switch_gap_inserted_between_different_speakers() {
        let segs = vec![seg(Speaker::Host, 10), seg(Speaker::Expert, 10)];
        let out = concat_with_gaps(&segs, SAMPLE_RATE);
        let switch_gap = (SAMPLE_RATE as u64 * GAP_SPEAKER_SWITCH_MS as u64 / 1000) as usize;
        assert_eq!(out.samples.len(), 10 + switch_gap + 10);
    }

    #[test]
    fn same_speaker_gap_is_shorter() {
        let segs = vec![seg(Speaker::Host, 10), seg(Speaker::Host, 10)];
        let out = concat_with_gaps(&segs, SAMPLE_RATE);
        let gap = (SAMPLE_RATE as u64 * GAP_SAME_SPEAKER_MS as u64 / 1000) as usize;
        assert_eq!(out.samples.len(), 10 + gap + 10);
    }

    #[test]
    fn gaps_scale_with_sample_rate() {
        let segs = vec![seg(Speaker::Host, 10), seg(Speaker::Expert, 10)];
        let out = concat_with_gaps(&segs, OPENAI_SAMPLE_RATE);
        let switch_gap = (OPENAI_SAMPLE_RATE as u64 * GAP_SPEAKER_SWITCH_MS as u64 / 1000) as usize;
        assert_eq!(out.samples.len(), 10 + switch_gap + 10);
    }

    #[test]
    fn encode_mp3_produces_nonempty_bytes() {
        let pcm = PcmAudio {
            samples: vec![0i16; SAMPLE_RATE as usize],
        };
        let mp3 = encode_mp3(&pcm, SAMPLE_RATE).unwrap();
        assert!(mp3.len() > 100);
        // MP3 frame sync: first byte 0xFF.
        assert_eq!(mp3[0], 0xFF);
    }

    #[test]
    fn encode_mp3_at_24khz() {
        let pcm = PcmAudio {
            samples: vec![0i16; OPENAI_SAMPLE_RATE as usize],
        };
        let mp3 = encode_mp3(&pcm, OPENAI_SAMPLE_RATE).unwrap();
        assert!(mp3.len() > 100);
        assert_eq!(mp3[0], 0xFF);
    }
}
