//! Hands-free voice coding session management.

use crate::{
    commands::{CommandParser, ControlAction, ParsedCommand, VoiceCommand},
    synthesizer::Synthesizer,
    transcriber::Transcriber,
    AudioRecording, FeedbackSound, Result, VoiceError,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Session configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Language for the session.
    pub language: String,

    /// Enable continuous listening mode.
    pub continuous: bool,

    /// Wake word to activate listening.
    pub wake_word: Option<String>,

    /// Enable audio feedback sounds.
    pub audio_feedback: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            continuous: false,
            wake_word: Some("hey ember".to_string()),
            audio_feedback: true,
        }
    }
}

/// Session state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SessionState {
    /// Session is not started.
    #[default]
    Inactive,
    /// Waiting for wake word.
    WaitingForWakeWord,
    /// Ready to receive commands.
    Ready,
    /// Listening for voice input.
    Listening,
    /// Processing command.
    Processing,
    /// Executing command.
    Executing,
    /// Waiting for confirmation.
    WaitingForConfirmation,
    /// Paused.
    Paused,
    /// Error state.
    Error,
}

/// Session event.
#[derive(Debug, Clone)]
pub enum SessionEvent {
    /// Session started.
    Started,
    /// Session stopped.
    Stopped,
    /// State changed.
    StateChanged(SessionState),
    /// Wake word detected.
    WakeWordDetected,
    /// Command recognized.
    CommandRecognized(ParsedCommand),
    /// Command executed.
    CommandExecuted {
        command: VoiceCommand,
        success: bool,
        result: Option<String>,
    },
    /// Error occurred.
    Error(String),
    /// Confirmation requested.
    ConfirmationRequested(String),
    /// Confirmation received.
    ConfirmationReceived(bool),
}

/// Command execution result.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Whether execution succeeded.
    pub success: bool,

    /// Output message.
    pub output: Option<String>,

    /// Error message if failed.
    pub error: Option<String>,

    /// Execution duration.
    pub duration: Duration,
}

/// Hands-free coding session.
pub struct HandsFreeSession {
    config: SessionConfig,
    state: SessionState,
    transcriber: Arc<Transcriber>,
    synthesizer: Arc<Synthesizer>,
    command_parser: Arc<CommandParser>,
    pending_command: Option<ParsedCommand>,
    event_tx: Option<mpsc::Sender<SessionEvent>>,
    command_history: Vec<ParsedCommand>,
    active: bool,
}

impl HandsFreeSession {
    /// Create a new hands-free session.
    pub async fn new(
        config: SessionConfig,
        transcriber: Arc<Transcriber>,
        synthesizer: Arc<Synthesizer>,
        command_parser: Arc<CommandParser>,
    ) -> Result<Self> {
        Ok(Self {
            config,
            state: SessionState::Inactive,
            transcriber,
            synthesizer,
            command_parser,
            pending_command: None,
            event_tx: None,
            command_history: Vec::new(),
            active: false,
        })
    }

    /// Start the session.
    pub async fn start(&mut self) -> Result<()> {
        if self.active {
            return Ok(());
        }

        self.active = true;

        if self.config.audio_feedback {
            self.synthesizer
                .play_feedback(FeedbackSound::SessionStart)
                .await?;
        }

        self.set_state(if self.config.wake_word.is_some() {
            SessionState::WaitingForWakeWord
        } else {
            SessionState::Ready
        })
        .await;

        self.emit_event(SessionEvent::Started).await;

        tracing::info!("Voice session started");

        Ok(())
    }

    /// Stop the session.
    pub async fn stop(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }

        self.active = false;

        if self.config.audio_feedback {
            self.synthesizer
                .play_feedback(FeedbackSound::SessionEnd)
                .await?;
        }

        self.set_state(SessionState::Inactive).await;
        self.emit_event(SessionEvent::Stopped).await;

        tracing::info!("Voice session stopped");

        Ok(())
    }

    /// Check if session is active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Get current state.
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Set state and emit event.
    async fn set_state(&mut self, state: SessionState) {
        if self.state != state {
            self.state = state;
            self.emit_event(SessionEvent::StateChanged(state)).await;
        }
    }

    /// Subscribe to session events.
    pub fn subscribe(&mut self) -> mpsc::Receiver<SessionEvent> {
        let (tx, rx) = mpsc::channel(100);
        self.event_tx = Some(tx);
        rx
    }

    /// Emit an event.
    async fn emit_event(&self, event: SessionEvent) {
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(event).await;
        }
    }

    /// Process voice input.
    pub async fn process_input(&mut self, audio: &AudioRecording) -> Result<Option<ParsedCommand>> {
        if !self.active {
            return Err(VoiceError::Session("Session not active".to_string()));
        }

        self.set_state(SessionState::Processing).await;

        if self.config.audio_feedback {
            self.synthesizer
                .play_feedback(FeedbackSound::Processing)
                .await?;
        }

        // Transcribe audio
        let result = self.transcriber.transcribe(audio).await?;
        let text = result.text.trim().to_lowercase();

        if text.is_empty() {
            self.set_state(SessionState::Ready).await;
            return Ok(None);
        }

        tracing::debug!("Transcribed: {}", text);

        // Check for wake word if waiting
        if self.state == SessionState::WaitingForWakeWord {
            if let Some(ref wake_word) = self.config.wake_word {
                if text.contains(&wake_word.to_lowercase()) {
                    self.emit_event(SessionEvent::WakeWordDetected).await;
                    self.set_state(SessionState::Ready).await;

                    if self.config.audio_feedback {
                        self.synthesizer.play_feedback(FeedbackSound::Ready).await?;
                    }

                    return Ok(None);
                }
            }
            return Ok(None);
        }

        // Parse command
        let command = self.command_parser.parse(&result.text)?;

        // Handle control commands immediately
        if let VoiceCommand::Control { action } = &command.command {
            return self.handle_control_action(*action).await;
        }

        // Handle confirmation if waiting
        if self.state == SessionState::WaitingForConfirmation {
            if let VoiceCommand::Confirm { confirmed } = &command.command {
                return self.handle_confirmation(*confirmed).await;
            }
        }

        // Check if command needs confirmation
        if command.is_dangerous() {
            self.pending_command = Some(command.clone());
            self.set_state(SessionState::WaitingForConfirmation).await;

            if let Some(ref prompt) = command.confirmation_prompt {
                self.emit_event(SessionEvent::ConfirmationRequested(prompt.clone()))
                    .await;
                self.synthesizer.speak(prompt).await?;
            }

            return Ok(Some(command));
        }

        self.emit_event(SessionEvent::CommandRecognized(command.clone()))
            .await;

        if self.config.audio_feedback {
            self.synthesizer
                .play_feedback(FeedbackSound::Success)
                .await?;
        }

        self.command_history.push(command.clone());
        self.set_state(SessionState::Ready).await;

        Ok(Some(command))
    }

    /// Handle control action.
    async fn handle_control_action(
        &mut self,
        action: ControlAction,
    ) -> Result<Option<ParsedCommand>> {
        match action {
            ControlAction::Stop | ControlAction::Exit => {
                self.stop().await?;
                Ok(None)
            }
            ControlAction::Cancel => {
                self.pending_command = None;
                self.set_state(SessionState::Ready).await;
                self.synthesizer.speak("Cancelled").await?;
                Ok(None)
            }
            ControlAction::Pause => {
                self.set_state(SessionState::Paused).await;
                self.synthesizer.speak("Paused").await?;
                Ok(None)
            }
            ControlAction::Resume => {
                self.set_state(SessionState::Ready).await;
                self.synthesizer.speak("Resumed").await?;
                Ok(None)
            }
            ControlAction::Undo => {
                // Would integrate with Ember's checkpoint system
                self.synthesizer.speak("Undo not implemented").await?;
                Ok(None)
            }
            ControlAction::Redo => {
                self.synthesizer.speak("Redo not implemented").await?;
                Ok(None)
            }
            ControlAction::Clear => {
                self.command_history.clear();
                self.synthesizer.speak("History cleared").await?;
                Ok(None)
            }
        }
    }

    /// Handle confirmation response.
    async fn handle_confirmation(&mut self, confirmed: bool) -> Result<Option<ParsedCommand>> {
        self.emit_event(SessionEvent::ConfirmationReceived(confirmed))
            .await;

        let command = self.pending_command.take();

        if confirmed {
            if let Some(cmd) = command {
                if self.config.audio_feedback {
                    self.synthesizer
                        .play_feedback(FeedbackSound::Success)
                        .await?;
                }
                self.command_history.push(cmd.clone());
                self.set_state(SessionState::Ready).await;
                return Ok(Some(cmd));
            }
        } else {
            self.synthesizer.speak("Cancelled").await?;
        }

        self.set_state(SessionState::Ready).await;
        Ok(None)
    }

    /// Execute a command and handle the result.
    pub async fn execute_command(
        &mut self,
        command: &ParsedCommand,
        executor: impl FnOnce(&VoiceCommand) -> ExecutionResult,
    ) -> Result<ExecutionResult> {
        self.set_state(SessionState::Executing).await;

        let result = executor(&command.command);

        let success = result.success;
        let output = result.output.clone();

        self.emit_event(SessionEvent::CommandExecuted {
            command: command.command.clone(),
            success,
            result: output.clone(),
        })
        .await;

        if self.config.audio_feedback {
            if success {
                self.synthesizer
                    .play_feedback(FeedbackSound::Success)
                    .await?;
            } else {
                self.synthesizer.play_feedback(FeedbackSound::Error).await?;
            }
        }

        // Speak result if available
        if let Some(ref msg) = output {
            if msg.len() < 200 {
                self.synthesizer.speak(msg).await?;
            }
        }

        self.set_state(SessionState::Ready).await;

        Ok(result)
    }

    /// Run the main session loop.
    pub async fn run_loop<F, Fut>(&mut self, mut record_fn: F) -> Result<()>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<AudioRecording>>,
    {
        while self.active {
            // Skip if paused
            if self.state == SessionState::Paused {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }

            // Set state to listening
            self.set_state(SessionState::Listening).await;

            // Record audio
            match record_fn().await {
                Ok(audio) => {
                    if !audio.is_empty() {
                        if let Err(e) = self.process_input(&audio).await {
                            tracing::error!("Error processing input: {}", e);
                            self.emit_event(SessionEvent::Error(e.to_string())).await;

                            if self.config.audio_feedback {
                                let _ = self
                                    .synthesizer
                                    .play_feedback(FeedbackSound::Error)
                                    .await;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Error recording: {}", e);
                    self.emit_event(SessionEvent::Error(e.to_string())).await;
                }
            }

            // Small delay between iterations
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        Ok(())
    }

    /// Get command history.
    pub fn history(&self) -> &[ParsedCommand] {
        &self.command_history
    }

    /// Clear command history.
    pub fn clear_history(&mut self) {
        self.command_history.clear();
    }

    /// Get session statistics.
    pub fn stats(&self) -> SessionStats {
        SessionStats {
            commands_processed: self.command_history.len(),
            is_active: self.active,
            current_state: self.state,
        }
    }
}

/// Session statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStats {
    /// Number of commands processed.
    pub commands_processed: usize,

    /// Whether session is active.
    pub is_active: bool,

    /// Current session state.
    pub current_state: SessionState,
}

/// Session builder for easier configuration.
pub struct SessionBuilder {
    config: SessionConfig,
    transcriber: Option<Arc<Transcriber>>,
    synthesizer: Option<Arc<Synthesizer>>,
    command_parser: Option<Arc<CommandParser>>,
}

impl SessionBuilder {
    /// Create a new session builder.
    pub fn new() -> Self {
        Self {
            config: SessionConfig::default(),
            transcriber: None,
            synthesizer: None,
            command_parser: None,
        }
    }

    /// Set the language.
    pub fn language(mut self, language: &str) -> Self {
        self.config.language = language.to_string();
        self
    }

    /// Enable continuous mode.
    pub fn continuous(mut self, enabled: bool) -> Self {
        self.config.continuous = enabled;
        self
    }

    /// Set wake word.
    pub fn wake_word(mut self, wake_word: Option<String>) -> Self {
        self.config.wake_word = wake_word;
        self
    }

    /// Enable audio feedback.
    pub fn audio_feedback(mut self, enabled: bool) -> Self {
        self.config.audio_feedback = enabled;
        self
    }

    /// Set transcriber.
    pub fn transcriber(mut self, transcriber: Arc<Transcriber>) -> Self {
        self.transcriber = Some(transcriber);
        self
    }

    /// Set synthesizer.
    pub fn synthesizer(mut self, synthesizer: Arc<Synthesizer>) -> Self {
        self.synthesizer = Some(synthesizer);
        self
    }

    /// Set command parser.
    pub fn command_parser(mut self, parser: Arc<CommandParser>) -> Self {
        self.command_parser = Some(parser);
        self
    }

    /// Build the session.
    pub async fn build(self) -> Result<HandsFreeSession> {
        let transcriber = self
            .transcriber
            .ok_or_else(|| VoiceError::Config("Transcriber not set".to_string()))?;
        let synthesizer = self
            .synthesizer
            .ok_or_else(|| VoiceError::Config("Synthesizer not set".to_string()))?;
        let command_parser = self
            .command_parser
            .ok_or_else(|| VoiceError::Config("Command parser not set".to_string()))?;

        HandsFreeSession::new(self.config, transcriber, synthesizer, command_parser).await
    }
}

impl Default for SessionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SttConfig, SttProvider, TtsConfig, TtsProvider};

    async fn create_test_session() -> HandsFreeSession {
        let stt_config = SttConfig {
            provider: SttProvider::Mock,
            ..Default::default()
        };
        let tts_config = TtsConfig {
            provider: TtsProvider::Mock,
            ..Default::default()
        };

        let transcriber = Arc::new(Transcriber::new(stt_config).await.unwrap());
        let synthesizer = Arc::new(Synthesizer::new(tts_config).await.unwrap());
        let command_parser = Arc::new(CommandParser::new("en".to_string()));

        HandsFreeSession::new(
            SessionConfig::default(),
            transcriber,
            synthesizer,
            command_parser,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn test_session_lifecycle() {
        let mut session = create_test_session().await;

        assert!(!session.is_active());
        assert_eq!(session.state(), SessionState::Inactive);

        session.start().await.unwrap();
        assert!(session.is_active());

        session.stop().await.unwrap();
        assert!(!session.is_active());
        assert_eq!(session.state(), SessionState::Inactive);
    }

    #[tokio::test]
    async fn test_session_stats() {
        let session = create_test_session().await;
        let stats = session.stats();

        assert_eq!(stats.commands_processed, 0);
        assert!(!stats.is_active);
    }

    #[test]
    fn test_session_config_default() {
        let config = SessionConfig::default();

        assert_eq!(config.language, "en");
        assert!(!config.continuous);
        assert!(config.audio_feedback);
        assert_eq!(config.wake_word, Some("hey ember".to_string()));
    }

    #[test]
    fn test_session_state_default() {
        assert_eq!(SessionState::default(), SessionState::Inactive);
    }

    #[tokio::test]
    async fn test_session_builder() {
        let stt_config = SttConfig {
            provider: SttProvider::Mock,
            ..Default::default()
        };
        let tts_config = TtsConfig {
            provider: TtsProvider::Mock,
            ..Default::default()
        };

        let transcriber = Arc::new(Transcriber::new(stt_config).await.unwrap());
        let synthesizer = Arc::new(Synthesizer::new(tts_config).await.unwrap());
        let command_parser = Arc::new(CommandParser::new("en".to_string()));

        let session = SessionBuilder::new()
            .language("de")
            .continuous(true)
            .wake_word(Some("hallo ember".to_string()))
            .audio_feedback(false)
            .transcriber(transcriber)
            .synthesizer(synthesizer)
            .command_parser(command_parser)
            .build()
            .await
            .unwrap();

        assert!(!session.is_active());
    }

    #[tokio::test]
    async fn test_history_management() {
        let mut session = create_test_session().await;

        assert!(session.history().is_empty());

        session.clear_history();
        assert!(session.history().is_empty());
    }
}