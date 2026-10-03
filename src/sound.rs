use rodio::{ChannelCount, Decoder, DeviceSinkBuilder, MixerDeviceSink, SampleRate, Source, buffer::SamplesBuffer};
use std::f32::consts::TAU;
use std::fs::File;
use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const RATE: u32 = 44_100;
const VOLUME: f32 = 0.22;
const MAX_CUSTOM_SECS: u32 = 4;
pub const EXTS: &[&str] = &["wav", "mp3", "ogg", "flac"];

/// Short synthesized UI sounds; the audio device opens on first use.
#[derive(Default)]
pub struct Sounds {
    sink: Option<MixerDeviceSink>,
    failed: bool,
    custom: Option<(PathBuf, Option<Clip>)>,
}

#[derive(Clone)]
struct Clip {
    channels: ChannelCount,
    rate: SampleRate,
    samples: Vec<f32>,
}

impl Sounds {
    /// A watery "bloop": `pitch` is the base frequency in Hz (bigger bubbles sound lower).
    pub fn bubble(&mut self, pitch: f32) {
        let Some(sink) = self.sink() else { return };
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
        let jitter = (nanos % 1000) as f32 / 1000.0;
        let samples = bubble_samples(pitch * (0.94 + 0.12 * jitter));
        sink.mixer().add(SamplesBuffer::new(NonZero::<u16>::MIN, NonZero::new(RATE).unwrap(), samples));
    }

    pub fn swap(&mut self) {
        let Some(sink) = self.sink() else { return };
        sink.mixer().add(SamplesBuffer::new(NonZero::<u16>::MIN, NonZero::new(RATE).unwrap(), swap_samples()));
    }

    pub fn custom(&mut self, path: &Path) -> bool {
        if self.custom.as_ref().is_none_or(|(p, _)| p != path) {
            self.custom = Some((path.to_owned(), decode(path)));
        }
        let Some(clip) = self.custom.as_ref().and_then(|(_, c)| c.clone()) else { return false };
        let Some(sink) = self.sink() else { return true };
        sink.mixer().add(SamplesBuffer::new(clip.channels, clip.rate, clip.samples));
        true
    }

    fn sink(&mut self) -> Option<&MixerDeviceSink> {
        if self.sink.is_none() && !self.failed {
            match DeviceSinkBuilder::open_default_sink() {
                Ok(mut sink) => {
                    sink.log_on_drop(false);
                    self.sink = Some(sink);
                }
                Err(e) => {
                    eprintln!("sounds disabled: {e}");
                    self.failed = true;
                }
            }
        }
        self.sink.as_ref()
    }
}

fn decode(path: &Path) -> Option<Clip> {
    let decoder = File::open(path).ok().and_then(|f| Decoder::try_from(f).ok());
    let Some(decoder) = decoder else {
        eprintln!("could not decode sound {}", path.display());
        return None;
    };
    let (channels, rate) = (decoder.channels(), decoder.sample_rate());
    let max = (rate.get() * MAX_CUSTOM_SECS) as usize * usize::from(channels.get());
    let samples: Vec<f32> = decoder.take(max).collect();
    (!samples.is_empty()).then_some(Clip { channels, rate, samples })
}

fn swap_samples() -> Vec<f32> {
    let mut out = vec![0.0; (RATE as f32 * 0.09) as usize];
    let mut phase = 0.0f32;
    for (n, sample) in out.iter_mut().enumerate() {
        let t = n as f32 / RATE as f32;
        let f = 1500.0 - 600.0 * (1.0 - (-t / 0.012).exp());
        phase = (phase + TAU * f / RATE as f32) % TAU;
        let attack = (t / 0.0015).min(1.0);
        let tone = phase.sin() * (-t / 0.018).exp() + 0.3 * (phase * 3.0).sin() * (-t / 0.004).exp();
        *sample = tone * attack * 0.7 * VOLUME;
    }
    out
}

/// A rising sine with a quick decay (how a real bubble rings as it pops free),
/// followed by a smaller, higher droplet.
fn bubble_samples(f0: f32) -> Vec<f32> {
    let mut out = vec![0.0; (RATE as f32 * 0.26) as usize];
    ring(&mut out, 0, f0, 1.0, 0.045);
    ring(&mut out, (RATE as f32 * 0.055) as usize, f0 * 1.55, 0.35, 0.03);
    out
}

fn ring(out: &mut [f32], start: usize, f0: f32, gain: f32, decay: f32) {
    let mut phase = 0.0f32;
    for (n, sample) in out.iter_mut().skip(start).enumerate() {
        let t = n as f32 / RATE as f32;
        let f = f0 * (1.0 + 0.9 * (1.0 - (-t / 0.05).exp()));
        phase = (phase + TAU * f / RATE as f32) % TAU;
        let attack = (t / 0.004).min(1.0);
        let tone = phase.sin() + 0.15 * (phase * 2.0).sin() * (-t / 0.015).exp();
        *sample += tone * attack * (-t / decay).exp() * gain * VOLUME;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bubble_stays_in_range_and_fades_out() {
        let samples = bubble_samples(700.0);
        assert!(samples.iter().all(|s| s.abs() <= 1.0));
        assert!(samples.last().unwrap().abs() < 0.001);
    }

    #[test]
    fn swap_stays_in_range_and_fades_out() {
        let samples = swap_samples();
        assert!(samples.iter().all(|s| s.abs() <= 1.0));
        assert!(samples.last().unwrap().abs() < 0.001);
    }
}
