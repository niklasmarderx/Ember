//! Ember Voice Interface - Speech-to-text and text-to-speech for Ember AI.
//!
//! This crate provides voice interaction capabilities:
//! - Speech-to-text using OpenAI Whisper or local models
//! - Text-to-speech using OpenAI TTS or local synthesis
//! - Natural language command parsing
//! - Hands-free coding sessions
//!
//! # Example
//!
//! ```rust,ignore
//! use ember_voice::{VoiceInterface, VoiceConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = VoiceConfig::default();
//!     let voice = VoiceInterface::new(config).await?;
//!     
//!     // Start hands-free session
//!     voice.start_session().await?;
//!     
//!     Ok(())
//! }
//! ```

#![allow(dead_code)]
#![allow(unused_variables)]

pub mod commands;
pub mod session;
pub mod synthesizer;
pub mod transcriber;

use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

pub use commands::{CommandParser, ParsedCommand, VoiceCommand};
pub use session::{HandsFreeSession, SessionConfig, SessionState};
pub use synthesizer::{Synthesizer, TtsConfig, TtsProvider, Voice};
pub use transcriber::{SttConfig, SttProvider, TranscribeResult, Transcriber};

/// Voice interface errors.
#[derive(Debug, Error)]
pub enum VoiceError {
    #[error("Audio device error: {0}")]
    AudioDevice(String),

    #[error("Recording error: {0}")]
    Recording(String),

    #[error("Transcription error: {0}")]
    Transcription(String),

    #[error("Synthesis error: {0}")]
    Synthesis(String),

    #[error("Playback error: {0}")]
    Playback(String),

    #[error("Command parsing error: {0}")]
    CommandParsing(String),

    #[error("Session error: {0}")]
    Session(String),

    #[error("API error: {0}")]
    Api(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

/// Result type for voice operations.
pub type Result<T> = std::result::Result<T, VoiceError>;

/// Voice interface configuration.
#[derive(Debug, Clone)]
pub struct VoiceConfig {
    /// Speech-to-text configuration.
    pub stt: SttConfig,

    /// Text-to-speech configuration.
    pub tts: TtsConfig,

    /// Wake word to activate listening (e.g., "hey ember").
    pub wake_word: Option<String>,

    /// Language for voice interaction.
    pub language: String,

    /// Enable continuous listening mode.
    pub continuous_listening: bool,

    /// Silence threshold for end-of-speech detection (seconds).
    pub silence_threshold: f32,

    /// Maximum recording duration (seconds).
    pub max_recording_duration: f32,

    /// Enable audio feedback sounds.
    pub audio_feedback: bool,

    /// Confirmation mode (always, never, dangerous_only).
    pub confirmation_mode: ConfirmationMode,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            stt: SttConfig::default(),
            tts: TtsConfig::default(),
            wake_word: Some("hey ember".to_string()),
            language: "en".to_string(),
            continuous_listening: false,
            silence_threshold: 1.5,
            max_recording_duration: 30.0,
            audio_feedback: true,
            confirmation_mode: ConfirmationMode::DangerousOnly,
        }
    }
}

/// Confirmation mode for voice commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConfirmationMode {
    /// Always confirm before executing.
    Always,
    /// Never confirm (execute immediately).
    Never,
    /// Only confirm dangerous commands (file deletion, etc.).
    #[default]
    DangerousOnly,
}

/// Audio recording from microphone.
#[derive(Debug, Clone)]
pub struct AudioRecording {
    /// Audio samples (PCM).
    pub samples: Vec<f32>,

    /// Sample rate in Hz.
    pub sample_rate: u32,

    /// Number of channels.
    pub channels: u16,

    /// Duration in seconds.
    pub duration: f32,

    /// Recording timestamp.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl AudioRecording {
    /// Create a new audio recording.
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16) -> Self {
        let duration = samples.len() as f32 / (sample_rate as f32 * channels as f32);
        Self {
            samples,
            sample_rate,
            channels,
            duration,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Check if the recording is empty.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Get the number of samples.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Convert to WAV bytes.
    pub fn to_wav(&self) -> Result<Vec<u8>> {
        let spec = hound::WavSpec {
            channels: self.channels,
            sample_rate: self.sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut buffer = std::io::Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(&mut buffer, spec)
                .map_err(|e| VoiceError::Recording(e.to_string()))?;

            for sample in &self.samples {
                let sample_i16 = (sample * 32767.0) as i16;
                writer
                    .write_sample(sample_i16)
                    .map_err(|e| VoiceError::Recording(e.to_string()))?;
            }
            writer
                .finalize()
                .map_err(|e| VoiceError::Recording(e.to_string()))?;
        }

        Ok(buffer.into_inner())
    }
}

/// Voice interface state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VoiceState {
    /// Idle, not listening.
    #[default]
    Idle,
    /// Waiting for wake word.
    WaitingForWakeWord,
    /// Actively listening for command.
    Listening,
    /// Processing audio.
    Processing,
    /// Speaking response.
    Speaking,
    /// Error state.
    Error,
}

/// Main voice interface.
pub struct VoiceInterface {
    config: VoiceConfig,
    transcriber: Arc<Transcriber>,
    synthesizer: Arc<Synthesizer>,
    command_parser: Arc<CommandParser>,
    state: Arc<RwLock<VoiceState>>,
    session: Option<Arc<RwLock<HandsFreeSession>>>,
}

impl VoiceInterface {
    /// Create a new voice interface.
    pub async fn new(config: VoiceConfig) -> Result<Self> {
        let transcriber = Transcriber::new(config.stt.clone()).await?;
        let synthesizer = Synthesizer::new(config.tts.clone()).await?;
        let command_parser = CommandParser::new(config.language.clone());

        Ok(Self {
            config,
            transcriber: Arc::new(transcriber),
            synthesizer: Arc::new(synthesizer),
            command_parser: Arc::new(command_parser),
            state: Arc::new(RwLock::new(VoiceState::Idle)),
            session: None,
        })
    }

    /// Get current state.
    pub async fn state(&self) -> VoiceState {
        *self.state.read().await
    }

    /// Set state.
    async fn set_state(&self, state: VoiceState) {
        *self.state.write().await = state;
    }

    /// Record audio from microphone.
    pub async fn record(&self) -> Result<AudioRecording> {
        self.set_state(VoiceState::Listening).await;

        let recording = self.record_internal().await?;

        self.set_state(VoiceState::Processing).await;
        Ok(recording)
    }

    /// Internal recording implementation.
    async fn record_internal(&self) -> Result<AudioRecording> {
        use crossbeam_channel::{bounded, Receiver};
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Mutex;

        let sample_rate = 16000u32;
        let channels = 1u16;
        let max_samples =
            (self.config.max_recording_duration * sample_rate as f32 * channels as f32) as usize;
        let silence_samples = (self.config.silence_threshold * sample_rate as f32) as usize;

        let samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::with_capacity(max_samples)));
        let recording = Arc::new(AtomicBool::new(true));

        let (tx, rx): (_, Receiver<()>) = bounded(1);

        let samples_clone = samples.clone();
        let recording_clone = recording.clone();
        let silence_threshold = silence_samples;

        // Simulate recording (in real implementation, use cpal)
        tokio::spawn(async move {
            let mut silence_counter = 0;
            let mut total_samples = 0;

            while recording_clone.load(Ordering::Relaxed) && total_samples < max_samples {
                // In real implementation, read from audio device
                // For now, simulate with silence detection
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                // Simulate audio data
                let chunk_size = (sample_rate as usize / 10) * channels as usize;
                let chunk: Vec<f32> = vec![0.0; chunk_size];

                // Check for silence
                let is_silence = chunk.iter().all(|&s| s.abs() < 0.01);
                if is_silence {
                    silence_counter += chunk_size;
                } else {
                    silence_counter = 0;
                }

                if let Ok(mut s) = samples_clone.lock() {
                    s.extend(chunk);
                    total_samples = s.len();
                }

                // Stop on prolonged silence
                if silence_counter > silence_threshold && total_samples > sample_rate as usize {
                    break;
                }
            }

            recording_clone.store(false, Ordering::Relaxed);
            let _ = tx.send(());
        });

        // Wait for recording to complete
        let _ = rx.recv();

        let final_samples = samples.lock().unwrap().clone();

        Ok(AudioRecording::new(final_samples, sample_rate, channels))
    }

    /// Transcribe audio to text.
    pub async fn transcribe(&self, audio: &AudioRecording) -> Result<String> {
        let result = self.transcriber.transcribe(audio).await?;
        Ok(result.text)
    }

    /// Speak text using TTS.
    pub async fn speak(&self, text: &str) -> Result<()> {
        self.set_state(VoiceState::Speaking).await;

        self.synthesizer.speak(text).await?;

        self.set_state(VoiceState::Idle).await;
        Ok(())
    }

    /// Parse voice command from text.
    pub fn parse_command(&self, text: &str) -> Result<ParsedCommand> {
        self.command_parser.parse(text)
    }

    /// Process a complete voice interaction.
    pub async fn process_voice_input(&self) -> Result<ParsedCommand> {
        // Record audio
        let audio = self.record().await?;

        if audio.is_empty() {
            return Err(VoiceError::Recording("No audio recorded".to_string()));
        }

        // Transcribe
        let text = self.transcribe(&audio).await?;

        if text.trim().is_empty() {
            return Err(VoiceError::Transcription("No speech detected".to_string()));
        }

        tracing::info!("Transcribed: {}", text);

        // Parse command
        let command = self.parse_command(&text)?;

        // Confirm if needed
        if self.needs_confirmation(&command) {
            self.speak(&format!("Did you say: {}?", text)).await?;
            // In real implementation, wait for confirmation
        }

        Ok(command)
    }

    /// Check if command needs confirmation.
    fn needs_confirmation(&self, command: &ParsedCommand) -> bool {
        match self.config.confirmation_mode {
            ConfirmationMode::Always => true,
            ConfirmationMode::Never => false,
            ConfirmationMode::DangerousOnly => command.is_dangerous(),
        }
    }

    /// Start a hands-free coding session.
    pub async fn start_session(&mut self) -> Result<()> {
        let session_config = SessionConfig {
            language: self.config.language.clone(),
            continuous: self.config.continuous_listening,
            wake_word: self.config.wake_word.clone(),
            audio_feedback: self.config.audio_feedback,
        };

        let session = HandsFreeSession::new(
            session_config,
            self.transcriber.clone(),
            self.synthesizer.clone(),
            self.command_parser.clone(),
        )
        .await?;

        self.session = Some(Arc::new(RwLock::new(session)));

        if let Some(ref session) = self.session {
            session.write().await.start().await?;
        }

        Ok(())
    }

    /// Stop the hands-free session.
    pub async fn stop_session(&mut self) -> Result<()> {
        if let Some(ref session) = self.session {
            session.write().await.stop().await?;
        }
        self.session = None;
        Ok(())
    }

    /// Check if session is active.
    pub async fn is_session_active(&self) -> bool {
        if let Some(ref session) = self.session {
            session.read().await.is_active()
        } else {
            false
        }
    }

    /// Play feedback sound.
    pub async fn play_feedback(&self, feedback: FeedbackSound) -> Result<()> {
        if !self.config.audio_feedback {
            return Ok(());
        }

        self.synthesizer.play_feedback(feedback).await
    }
}

/// Feedback sounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackSound {
    /// Ready to listen.
    Ready,
    /// Command recognized.
    Success,
    /// Error occurred.
    Error,
    /// Processing.
    Processing,
    /// Session started.
    SessionStart,
    /// Session ended.
    SessionEnd,
}

/// Voice interaction event.
#[derive(Debug, Clone)]
pub enum VoiceEvent {
    /// State changed.
    StateChanged(VoiceState),
    /// Audio recording started.
    RecordingStarted,
    /// Audio recording completed.
    RecordingCompleted(AudioRecording),
    /// Transcription completed.
    TranscriptionCompleted(String),
    /// Command parsed.
    CommandParsed(ParsedCommand),
    /// Speech started.
    SpeechStarted,
    /// Speech completed.
    SpeechCompleted,
    /// Error occurred.
    Error(String),
}

/// Voice interaction statistics.
#[derive(Debug, Clone, Default)]
pub struct VoiceStats {
    /// Total recordings.
    pub total_recordings: u64,
    /// Total transcriptions.
    pub total_transcriptions: u64,
    /// Total speech synthesis.
    pub total_speech: u64,
    /// Total recording duration (seconds).
    pub total_recording_duration: f64,
    /// Total speech duration (seconds).
    pub total_speech_duration: f64,
    /// Average transcription confidence.
    pub avg_confidence: f64,
    /// Commands recognized.
    pub commands_recognized: u64,
    /// Commands executed.
    pub commands_executed: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_recording() {
        let samples = vec![0.0f32; 16000];
        let recording = AudioRecording::new(samples, 16000, 1);

        assert_eq!(recording.duration, 1.0);
        assert_eq!(recording.sample_rate, 16000);
        assert_eq!(recording.channels, 1);
    }

    #[test]
    fn test_voice_config_default() {
        let config = VoiceConfig::default();

        assert_eq!(config.language, "en");
        assert_eq!(config.wake_word, Some("hey ember".to_string()));
        assert!(!config.continuous_listening);
        assert!(config.audio_feedback);
    }

    #[test]
    fn test_confirmation_mode() {
        assert_eq!(ConfirmationMode::default(), ConfirmationMode::DangerousOnly);
    }

    #[test]
    fn test_voice_state_default() {
        assert_eq!(VoiceState::default(), VoiceState::Idle);
    }

    #[test]
    fn test_audio_to_wav() {
        let samples = vec![0.0f32; 1600]; // 0.1 seconds at 16kHz
        let recording = AudioRecording::new(samples, 16000, 1);

        let wav = recording.to_wav().unwrap();
        assert!(!wav.is_empty());

        // WAV header should start with "RIFF"
        assert_eq!(&wav[0..4], b"RIFF");
    }
}
