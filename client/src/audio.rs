#[cfg(feature = "audio")]
use rodio::{OutputStream, Sink, Source};
#[cfg(feature = "audio")]
use std::time::Duration;

#[cfg(feature = "audio")]
pub struct AudioPlayer {
    _stream: OutputStream,
    sink: Sink,
}

#[cfg(feature = "audio")]
impl AudioPlayer {
    pub fn new() -> Option<Self> {
        let (stream, stream_handle) = OutputStream::try_default().ok()?;
        let sink = Sink::try_new(&stream_handle).ok()?;
        Some(Self {
            _stream: stream,
            sink,
        })
    }

    pub fn beep(&self, frequency: f32, duration_ms: u64) {
        let source = SineWave::new(frequency)
            .take_duration(Duration::from_millis(duration_ms))
            .amplify(0.2);
        self.sink.append(source);
    }

    pub fn beep_for_trades(&self, trades_per_second: f32) {
        let frequency = 200.0 + (trades_per_second.min(50.0) * 16.0);
        let duration = 50;
        self.beep(frequency, duration);
    }
}

#[cfg(feature = "audio")]
struct SineWave {
    freq: f32,
    sample_rate: u32,
    num_sample: usize,
}

#[cfg(feature = "audio")]
impl SineWave {
    fn new(freq: f32) -> Self {
        Self {
            freq,
            sample_rate: 44100,
            num_sample: 0,
        }
    }
}

#[cfg(feature = "audio")]
impl Iterator for SineWave {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        self.num_sample = self.num_sample.wrapping_add(1);
        let t = self.num_sample as f32 / self.sample_rate as f32;
        Some((t * self.freq * 2.0 * std::f32::consts::PI).sin())
    }
}

#[cfg(feature = "audio")]
impl Source for SineWave {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        1
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[cfg(not(feature = "audio"))]
pub struct AudioPlayer;

#[cfg(not(feature = "audio"))]
impl AudioPlayer {
    pub fn new() -> Option<Self> {
        Some(Self)
    }

    pub fn beep(&self, _frequency: f32, _duration_ms: u64) {}

    pub fn beep_for_trades(&self, _trades_per_second: f32) {}
}
