//! Speech-to-text transcription using OpenAI Whisper or local models.

use crate::{AudioRecording, Result, VoiceError};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Speech-to-text provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SttProvider {
    /// OpenAI Whisper API.
    #[default]
    OpenAiWhisper,
    /// Local Whisper model (whisper.cpp).
    LocalWhisper,
    /// Azure Speech Services.
    AzureSpeech,
    /// Google Cloud Speech-to-Text.
    GoogleSpeech,
    /// Mock provider for testing.
    Mock,
}

/// Speech-to-text configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttConfig {
    /// Provider to use.
    pub provider: SttProvider,

    /// API key for cloud providers.
    pub api_key: Option<String>,

    /// Model to use (e.g., "whisper-1").
    pub model: String,

    /// Language code (e.g., "en", "de").
    pub language: Option<String>,

    /// Enable automatic punctuation.
    pub punctuate: bool,

    /// Enable profanity filtering.
    pub profanity_filter: bool,

    /// Request timeout.
    pub timeout: Duration,

    /// Custom API endpoint.
    pub endpoint: Option<String>,

    /// Local model path (for LocalWhisper).
    pub model_path: Option<String>,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            provider: SttProvider::OpenAiWhisper,
            api_key: std::env::var("OPENAI_API_KEY").ok(),
            model: "whisper-1".to_string(),
            language: None,
            punctuate: true,
            profanity_filter: false,
            timeout: Duration::from_secs(30),
            endpoint: None,
            model_path: None,
        }
    }
}

/// Transcription result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscribeResult {
    /// Transcribed text.
    pub text: String,

    /// Confidence score (0.0 - 1.0).
    pub confidence: f32,

    /// Detected language.
    pub language: Option<String>,

    /// Word-level timestamps.
    pub words: Vec<WordTimestamp>,

    /// Processing duration.
    pub processing_time: Duration,

    /// Audio duration.
    pub audio_duration: Duration,
}

/// Word with timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordTimestamp {
    /// The word.
    pub word: String,

    /// Start time in seconds.
    pub start: f32,

    /// End time in seconds.
    pub end: f32,

    /// Confidence for this word.
    pub confidence: f32,
}

/// Speech-to-text transcriber.
pub struct Transcriber {
    config: SttConfig,
    client: reqwest::Client,
}

impl Transcriber {
    /// Create a new transcriber.
    pub async fn new(config: SttConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| VoiceError::Config(e.to_string()))?;

        Ok(Self { config, client })
    }

    /// Transcribe audio to text.
    pub async fn transcribe(&self, audio: &AudioRecording) -> Result<TranscribeResult> {
        let start = std::time::Instant::now();

        let result = match self.config.provider {
            SttProvider::OpenAiWhisper => self.transcribe_openai(audio).await?,
            SttProvider::LocalWhisper => self.transcribe_local(audio).await?,
            SttProvider::AzureSpeech => self.transcribe_azure(audio).await?,
            SttProvider::GoogleSpeech => self.transcribe_google(audio).await?,
            SttProvider::Mock => self.transcribe_mock(audio).await?,
        };

        let mut result = result;
        result.processing_time = start.elapsed();
        result.audio_duration = Duration::from_secs_f32(audio.duration);

        tracing::debug!(
            "Transcribed {} seconds of audio in {:?}: {}",
            audio.duration,
            result.processing_time,
            result.text
        );

        Ok(result)
    }

    /// Transcribe using OpenAI Whisper API.
    async fn transcribe_openai(&self, audio: &AudioRecording) -> Result<TranscribeResult> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| VoiceError::Config("OpenAI API key not configured".to_string()))?;

        let endpoint = self
            .config
            .endpoint
            .as_deref()
            .unwrap_or("https://api.openai.com/v1/audio/transcriptions");

        // Convert audio to WAV
        let wav_data = audio.to_wav()?;

        // Build multipart form
        let file_part = reqwest::multipart::Part::bytes(wav_data)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| VoiceError::Api(e.to_string()))?;

        let mut form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .text("model", self.config.model.clone())
            .text("response_format", "verbose_json");

        if let Some(ref lang) = self.config.language {
            form = form.text("language", lang.clone());
        }

        // Send request
        let response = self
            .client
            .post(endpoint)
            .bearer_auth(api_key)
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(VoiceError::Api(format!("OpenAI API error: {}", error_text)));
        }

        let response_json: WhisperResponse = response
            .json()
            .await
            .map_err(|e| VoiceError::Api(e.to_string()))?;

        // Parse words with timestamps
        let words = response_json
            .words
            .unwrap_or_default()
            .into_iter()
            .map(|w| WordTimestamp {
                word: w.word,
                start: w.start,
                end: w.end,
                confidence: 0.95, // Whisper doesn't provide per-word confidence
            })
            .collect();

        Ok(TranscribeResult {
            text: response_json.text,
            confidence: 0.95, // Whisper doesn't provide overall confidence
            language: response_json.language,
            words,
            processing_time: Duration::ZERO,
            audio_duration: Duration::ZERO,
        })
    }

    /// Transcribe using local Whisper model.
    async fn transcribe_local(&self, audio: &AudioRecording) -> Result<TranscribeResult> {
        // In a real implementation, this would use whisper.cpp or similar
        // For now, return a placeholder
        tracing::warn!("Local Whisper not implemented, using mock");
        self.transcribe_mock(audio).await
    }

    /// Transcribe using Azure Speech Services.
    async fn transcribe_azure(&self, audio: &AudioRecording) -> Result<TranscribeResult> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| VoiceError::Config("Azure API key not configured".to_string()))?;

        let endpoint = self
            .config
            .endpoint
            .as_deref()
            .ok_or_else(|| VoiceError::Config("Azure endpoint not configured".to_string()))?;

        let wav_data = audio.to_wav()?;

        let language = self.config.language.as_deref().unwrap_or("en-US");

        let url = format!(
            "{}/speech/recognition/conversation/cognitiveservices/v1?language={}",
            endpoint, language
        );

        let response = self
            .client
            .post(&url)
            .header("Ocp-Apim-Subscription-Key", api_key)
            .header("Content-Type", "audio/wav")
            .body(wav_data)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(VoiceError::Api(format!("Azure API error: {}", error_text)));
        }

        let response_json: AzureSpeechResponse = response
            .json()
            .await
            .map_err(|e| VoiceError::Api(e.to_string()))?;

        Ok(TranscribeResult {
            text: response_json.display_text,
            confidence: response_json
                .n_best
                .first()
                .map(|n| n.confidence)
                .unwrap_or(0.0),
            language: Some(language.to_string()),
            words: vec![],
            processing_time: Duration::ZERO,
            audio_duration: Duration::ZERO,
        })
    }

    /// Transcribe using Google Cloud Speech-to-Text.
    async fn transcribe_google(&self, audio: &AudioRecording) -> Result<TranscribeResult> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| VoiceError::Config("Google API key not configured".to_string()))?;

        let wav_data = audio.to_wav()?;
        let audio_content =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &wav_data);

        let language = self.config.language.as_deref().unwrap_or("en-US");

        let request_body = serde_json::json!({
            "config": {
                "encoding": "LINEAR16",
                "sampleRateHertz": audio.sample_rate,
                "languageCode": language,
                "enableAutomaticPunctuation": self.config.punctuate,
                "profanityFilter": self.config.profanity_filter,
            },
            "audio": {
                "content": audio_content
            }
        });

        let url = format!(
            "https://speech.googleapis.com/v1/speech:recognize?key={}",
            api_key
        );

        let response = self.client.post(&url).json(&request_body).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(VoiceError::Api(format!("Google API error: {}", error_text)));
        }

        let response_json: GoogleSpeechResponse = response
            .json()
            .await
            .map_err(|e| VoiceError::Api(e.to_string()))?;

        let result = response_json
            .results
            .first()
            .and_then(|r| r.alternatives.first());

        Ok(TranscribeResult {
            text: result.map(|r| r.transcript.clone()).unwrap_or_default(),
            confidence: result.map(|r| r.confidence).unwrap_or(0.0),
            language: Some(language.to_string()),
            words: vec![],
            processing_time: Duration::ZERO,
            audio_duration: Duration::ZERO,
        })
    }

    /// Mock transcription for testing.
    async fn transcribe_mock(&self, audio: &AudioRecording) -> Result<TranscribeResult> {
        // Simulate processing delay
        tokio::time::sleep(Duration::from_millis(100)).await;

        Ok(TranscribeResult {
            text: "This is a mock transcription for testing purposes.".to_string(),
            confidence: 0.99,
            language: Some("en".to_string()),
            words: vec![
                WordTimestamp {
                    word: "This".to_string(),
                    start: 0.0,
                    end: 0.2,
                    confidence: 0.99,
                },
                WordTimestamp {
                    word: "is".to_string(),
                    start: 0.2,
                    end: 0.3,
                    confidence: 0.99,
                },
                WordTimestamp {
                    word: "a".to_string(),
                    start: 0.3,
                    end: 0.4,
                    confidence: 0.99,
                },
                WordTimestamp {
                    word: "mock".to_string(),
                    start: 0.4,
                    end: 0.6,
                    confidence: 0.99,
                },
            ],
            processing_time: Duration::from_millis(100),
            audio_duration: Duration::from_secs_f32(audio.duration),
        })
    }

    /// Get the current configuration.
    pub fn config(&self) -> &SttConfig {
        &self.config
    }

    /// Update the configuration.
    pub fn set_config(&mut self, config: SttConfig) {
        self.config = config;
    }
}

// OpenAI Whisper API response types
#[derive(Debug, Deserialize)]
struct WhisperResponse {
    text: String,
    language: Option<String>,
    duration: Option<f32>,
    words: Option<Vec<WhisperWord>>,
}

#[derive(Debug, Deserialize)]
struct WhisperWord {
    word: String,
    start: f32,
    end: f32,
}

// Azure Speech response types
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AzureSpeechResponse {
    recognition_status: String,
    display_text: String,
    #[serde(default)]
    n_best: Vec<AzureNBest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AzureNBest {
    confidence: f32,
    lexical: String,
    display: String,
}

// Google Speech response types
#[derive(Debug, Deserialize)]
struct GoogleSpeechResponse {
    #[serde(default)]
    results: Vec<GoogleSpeechResult>,
}

#[derive(Debug, Deserialize)]
struct GoogleSpeechResult {
    alternatives: Vec<GoogleSpeechAlternative>,
}

#[derive(Debug, Deserialize)]
struct GoogleSpeechAlternative {
    transcript: String,
    #[serde(default)]
    confidence: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stt_config_default() {
        let config = SttConfig::default();
        assert_eq!(config.provider, SttProvider::OpenAiWhisper);
        assert_eq!(config.model, "whisper-1");
        assert!(config.punctuate);
    }

    #[test]
    fn test_stt_provider_variants() {
        assert_eq!(SttProvider::default(), SttProvider::OpenAiWhisper);

        let providers = [
            SttProvider::OpenAiWhisper,
            SttProvider::LocalWhisper,
            SttProvider::AzureSpeech,
            SttProvider::GoogleSpeech,
            SttProvider::Mock,
        ];

        assert_eq!(providers.len(), 5);
    }

    #[tokio::test]
    async fn test_mock_transcription() {
        let config = SttConfig {
            provider: SttProvider::Mock,
            ..Default::default()
        };

        let transcriber = Transcriber::new(config).await.unwrap();

        let audio = AudioRecording::new(vec![0.0; 16000], 16000, 1);
        let result = transcriber.transcribe(&audio).await.unwrap();

        assert!(!result.text.is_empty());
        assert!(result.confidence > 0.9);
        assert_eq!(result.language, Some("en".to_string()));
    }

    #[test]
    fn test_word_timestamp() {
        let word = WordTimestamp {
            word: "hello".to_string(),
            start: 0.0,
            end: 0.5,
            confidence: 0.95,
        };

        assert_eq!(word.word, "hello");
        assert_eq!(word.end - word.start, 0.5);
    }
}
