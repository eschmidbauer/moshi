use crate::vad::{EnergyVad, VadConfig};

#[derive(Debug, Clone)]
pub struct TrackerConfig {
    pub vad: VadConfig,
    pub finalize_after_s: f64,
}

impl Default for TrackerConfig {
    fn default() -> Self {
        Self { vad: VadConfig::default(), finalize_after_s: 0.8 }
    }
}

#[derive(Debug, Clone)]
pub struct TranscriptUpdate {
    pub text: String,
    pub start_time: f64,
    pub stop_time: Option<f64>,
    pub is_final: bool,
}

#[derive(Debug, Clone)]
struct EmittedState {
    text: String,
    stop_time: Option<f64>,
    is_final: bool,
}

#[derive(Debug, Clone)]
pub struct TranscriptTracker {
    vad: EnergyVad,
    finalize_after_s: f64,
    buffer: String,
    start_time: Option<f64>,
    last_stop_time: Option<f64>,
    last_emitted: Option<EmittedState>,
}

impl TranscriptTracker {
    pub fn new(cfg: TrackerConfig) -> Self {
        Self {
            vad: EnergyVad::new(cfg.vad),
            finalize_after_s: cfg.finalize_after_s,
            buffer: String::new(),
            start_time: None,
            last_stop_time: None,
            last_emitted: None,
        }
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.start_time = None;
        self.last_stop_time = None;
        self.last_emitted = None;
        self.vad.reset();
    }

    pub fn ingest_audio(&mut self, pcm: &[f32]) -> Vec<TranscriptUpdate> {
        let mut updates = Vec::new();
        self.vad.process(pcm);
        if self.should_finalize_by_timer() {
            if let Some(update) = self.finalize(Some(self.vad.last_voice_time())) {
                updates.push(update);
            }
        }
        updates
    }

    pub fn handle_word(&mut self, word: &str, start_time: f64) -> Option<TranscriptUpdate> {
        let piece = word.trim();
        if piece.is_empty() {
            return None;
        }
        if !self.buffer.is_empty() {
            let prev_char = self.buffer.chars().rev().find(|c| !c.is_whitespace());
            let next_char = piece.chars().find(|c| !c.is_whitespace());
            let needs_separator = prev_char.map(|c| c.is_alphanumeric()).unwrap_or(false)
                && next_char.map(|c| c.is_alphanumeric()).unwrap_or(false);
            if needs_separator {
                self.buffer.push(' ');
            }
        }
        self.buffer.push_str(piece);
        self.start_time.get_or_insert(start_time);
        self.emit_partial()
    }

    pub fn handle_end_word(&mut self, stop_time: f64) -> Option<TranscriptUpdate> {
        self.last_stop_time = Some(stop_time);
        self.emit_partial()
    }

    pub fn force_finalize(&mut self) -> Option<TranscriptUpdate> {
        let fallback = if let Some(stop) = self.last_stop_time {
            Some(stop)
        } else {
            let t = self.vad.last_voice_time();
            if t > 0.0 {
                Some(t)
            } else {
                let current = self.vad.current_time();
                if current > 0.0 {
                    Some(current)
                } else {
                    None
                }
            }
        };
        self.finalize(fallback)
    }

    fn emit_partial(&mut self) -> Option<TranscriptUpdate> {
        let start_time = self.start_time?;
        let text = self.buffer.trim().to_string();
        if text.is_empty() {
            return None;
        }
        let stop_time = self.last_stop_time;
        let update =
            TranscriptUpdate { text: text.clone(), start_time, stop_time, is_final: false };
        if self.should_emit(&update) {
            self.last_emitted = Some(EmittedState { text, stop_time, is_final: false });
            return Some(update);
        }
        None
    }

    fn finalize(&mut self, fallback_stop: Option<f64>) -> Option<TranscriptUpdate> {
        let start_time = self.start_time?;
        let text = self.buffer.trim().to_string();
        if text.is_empty() {
            self.reset_segment_state();
            return None;
        }
        let mut stop_time = self.last_stop_time.or(fallback_stop);
        if let Some(stop) = stop_time {
            if stop < start_time {
                // Clamp to the segment start to avoid reporting negative-length ranges when
                // the VAD lag causes its timestamps to precede the ASR start_time.
                stop_time = Some(start_time);
            }
        }
        let update = TranscriptUpdate { text: text.clone(), start_time, stop_time, is_final: true };
        self.reset_segment_state();
        Some(update)
    }

    fn reset_segment_state(&mut self) {
        self.buffer.clear();
        self.start_time = None;
        self.last_stop_time = None;
        self.last_emitted = None;
    }

    fn should_emit(&self, update: &TranscriptUpdate) -> bool {
        match &self.last_emitted {
            Some(prev) => {
                prev.text != update.text
                    || prev.stop_time != update.stop_time
                    || prev.is_final != update.is_final
            }
            None => true,
        }
    }

    fn should_finalize_by_timer(&self) -> bool {
        if self.buffer.is_empty() {
            return false;
        }
        if self.vad.in_speech() {
            return false;
        }
        let last_voice_time = self.vad.last_voice_time();
        if last_voice_time <= 0.0 {
            return false;
        }
        let silence = self.vad.current_time() - last_voice_time;
        silence >= self.finalize_after_s
    }
}
