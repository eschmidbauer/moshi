use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct VadConfig {
    pub sample_rate: usize,
    pub frame_length: usize,
    pub energy_threshold: f32,
    pub min_silence_duration: f64,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            sample_rate: 24000,
            frame_length: 480,
            energy_threshold: 7.5e-4,
            min_silence_duration: 0.6,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VadEvent {
    SpeechEnded { end_time: f64 },
}

#[derive(Debug, Clone)]
pub struct EnergyVad {
    cfg: VadConfig,
    buffer: VecDeque<f32>,
    current_time: f64,
    in_speech: bool,
    silence_accumulator: f64,
    last_voice_time: f64,
}

impl EnergyVad {
    pub fn new(cfg: VadConfig) -> Self {
        Self {
            cfg,
            buffer: VecDeque::new(),
            current_time: 0.0,
            in_speech: false,
            silence_accumulator: 0.0,
            last_voice_time: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.current_time = 0.0;
        self.in_speech = false;
        self.silence_accumulator = 0.0;
        self.last_voice_time = 0.0;
    }

    pub fn current_time(&self) -> f64 {
        self.current_time
    }

    pub fn last_voice_time(&self) -> f64 {
        self.last_voice_time
    }

    pub fn in_speech(&self) -> bool {
        self.in_speech
    }

    pub fn process(&mut self, pcm: &[f32]) -> Vec<VadEvent> {
        if pcm.is_empty() {
            return vec![];
        }
        self.buffer.extend(pcm.iter().copied());
        let mut events = Vec::new();
        let frame_len = self.cfg.frame_length;
        if frame_len == 0 || self.cfg.sample_rate == 0 {
            return events;
        }
        let frame_duration = frame_len as f64 / self.cfg.sample_rate as f64;
        while self.buffer.len() >= frame_len {
            let mut energy = 0.0f32;
            for _ in 0..frame_len {
                if let Some(sample) = self.buffer.pop_front() {
                    energy += sample * sample;
                }
            }
            energy /= frame_len as f32;
            let next_time = self.current_time + frame_duration;
            if energy >= self.cfg.energy_threshold {
                self.in_speech = true;
                self.silence_accumulator = 0.0;
                self.last_voice_time = next_time;
            } else if self.in_speech {
                self.silence_accumulator += frame_duration;
                if self.silence_accumulator >= self.cfg.min_silence_duration {
                    self.in_speech = false;
                    self.silence_accumulator = 0.0;
                    events.push(VadEvent::SpeechEnded { end_time: self.last_voice_time });
                }
            }
            self.current_time = next_time;
        }
        events
    }
}
