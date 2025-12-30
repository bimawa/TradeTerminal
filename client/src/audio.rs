#[cfg(feature = "audio")]
use rodio::{OutputStream, Source};
#[cfg(feature = "audio")]
use std::sync::mpsc;
#[cfg(feature = "audio")]
use std::time::{Duration, Instant};

#[cfg(feature = "audio")]
pub struct AudioPlayer {
    tx: mpsc::Sender<AudioCommand>,
}

#[cfg(feature = "audio")]
enum AudioCommand {
    Tick(bool),
}

#[cfg(feature = "audio")]
impl AudioPlayer {
    pub fn new() -> Option<Self> {
        let (tx, rx) = mpsc::channel::<AudioCommand>();

        std::thread::spawn(move || {
            let Ok((stream, handle)) = OutputStream::try_default() else {
                return;
            };
            let _stream = stream;

            let mut trade_times: Vec<Instant> = Vec::new();
            let mut last_sound: Option<Instant> = None;
            let window = Duration::from_secs(1);
            let min_interval = Duration::from_millis(50);

            while let Ok(cmd) = rx.recv() {
                match cmd {
                    AudioCommand::Tick(is_buy) => {
                        let now = Instant::now();
                        trade_times.push(now);
                        trade_times.retain(|t| now.duration_since(*t) < window);

                        if let Some(last) = last_sound {
                            if now.duration_since(last) < min_interval {
                                continue;
                            }
                        }
                        last_sound = Some(now);

                        let trades_per_sec = trade_times.len() as f32;
                        let base_freq = if is_buy { 800.0 } else { 500.0 };
                        let frequency = base_freq + (trades_per_sec.min(50.0) * 10.0);

                        let source = SmoothTick::new(frequency, 20)
                            .amplify(0.03);
                        let _ = handle.play_raw(source.convert_samples());
                    }
                }
            }
        });

        Some(Self { tx })
    }

    pub fn tick(&self, side_is_buy: bool) {
        let _ = self.tx.send(AudioCommand::Tick(side_is_buy));
    }
}

#[cfg(feature = "audio")]
struct SmoothTick {
    freq: f32,
    sample_rate: u32,
    num_sample: usize,
    total_samples: usize,
}

#[cfg(feature = "audio")]
impl SmoothTick {
    fn new(freq: f32, duration_ms: u64) -> Self {
        let sample_rate = 44100u32;
        Self {
            freq,
            sample_rate,
            num_sample: 0,
            total_samples: (sample_rate as u64 * duration_ms / 1000) as usize,
        }
    }
}

#[cfg(feature = "audio")]
impl Iterator for SmoothTick {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.num_sample >= self.total_samples {
            return None;
        }

        let t = self.num_sample as f32 / self.sample_rate as f32;
        let progress = self.num_sample as f32 / self.total_samples as f32;

        let envelope = if progress < 0.1 {
            progress / 0.1
        } else {
            1.0 - ((progress - 0.1) / 0.9)
        };
        let envelope = envelope * envelope;

        let sine = (t * self.freq * 2.0 * std::f32::consts::PI).sin();

        self.num_sample += 1;
        Some(sine * envelope)
    }
}

#[cfg(feature = "audio")]
impl Source for SmoothTick {
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.total_samples - self.num_sample)
    }

    fn channels(&self) -> u16 {
        1
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_millis(
            (self.total_samples as u64 * 1000) / self.sample_rate as u64,
        ))
    }
}

#[cfg(not(feature = "audio"))]
pub struct AudioPlayer;

#[cfg(not(feature = "audio"))]
impl AudioPlayer {
    pub fn new() -> Option<Self> {
        Some(Self)
    }

    pub fn tick(&self, _side_is_buy: bool) {}
}
