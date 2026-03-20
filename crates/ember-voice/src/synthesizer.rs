//! Text-to-speech synthesis using OpenAI TTS or local models.

use crate::{FeedbackSound, Result, VoiceError};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Text-to-speech provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TtsProvider {
    /// OpenAI TTS API.
    #[default]
    OpenAiTts,
    /// Azure Speech Services.
    AzureTts,
    /// Google Cloud Text-to-Speech.
    GoogleTts,
    /// ElevenLabs API.
    ElevenLabs,
    /// Local TTS (espeak, piper).
    LocalTts,
    /// Mock provider for testing.
    Mock,
}

/// Voice selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Voice {
    /// Voice ID.
    pub id: String,

    /// Voice name.
    pub name: String,

    /// Language code.
    pub language: String,

    /// Gender.
    pub gender: VoiceGender,

    /// Voice style (if supported).
    pub style: Option<String>,
}

/// Voice gender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum VoiceGender {
    #[default]
    Neutral,
    Male,
    Female,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            id: "alloy".to_string(),
            name: "Alloy".to_string(),
            language: "en".to_string(),
            gender: VoiceGender::Neutral,
            style: None,
        }
    }
}

/// Text-to-speech configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsConfig {
    /// Provider to use.
    pub provider: TtsProvider,

    /// API key for cloud providers.
    pub api_key: Option<String>,

    /// Voice to use.
    pub voice: Voice,

    /// Speech rate (0.25 - 4.0, default 1.0).
    pub rate: f32,

    /// Pitch adjustment (-20 to 20 semitones, default 0).
    pub pitch: f32,

    /// Volume (0.0 - 1.0, default 1.0).
    pub volume: f32,

    /// Output format.
    pub format: AudioFormat,

    /// Request timeout.
    pub timeout: Duration,

    /// Custom API endpoint.
    pub endpoint: Option<String>,

    /// Cache synthesized audio.
    pub cache_audio: bool,

    /// Cache directory.
    pub cache_dir: Option<String>,
}

/// Audio output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AudioFormat {
    /// MP3 format.
    #[default]
    Mp3,
    /// WAV format.
    Wav,
    /// Opus format.
    Opus,
    /// AAC format.
    Aac,
    /// FLAC format.
    Flac,
    /// PCM (raw audio).
    Pcm,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            provider: TtsProvider::OpenAiTts,
            api_key: std::env::var("OPENAI_API_KEY").ok(),
            voice: Voice::default(),
            rate: 1.0,
            pitch: 0.0,
            volume: 1.0,
            format: AudioFormat::Mp3,
            timeout: Duration::from_secs(30),
            endpoint: None,
            cache_audio: true,
            cache_dir: None,
        }
    }
}

/// Synthesized audio result.
#[derive(Debug, Clone)]
pub struct SynthesisResult {
    /// Audio data.
    pub audio: Vec<u8>,

    /// Audio format.
    pub format: AudioFormat,

    /// Duration in seconds.
    pub duration: f32,

    /// Sample rate.
    pub sample_rate: u32,

    /// Processing time.
    pub processing_time: Duration,

    /// Character count of input text.
    pub character_count: usize,
}

/// Text-to-speech synthesizer.
pub struct Synthesizer {
    config: TtsConfig,
    client: reqwest::Client,
}

impl Synthesizer {
    /// Create a new synthesizer.
    pub async fn new(config: TtsConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| VoiceError::Config(e.to_string()))?;

        Ok(Self { config, client })
    }

    /// Synthesize text to speech.
    pub async fn synthesize(&self, text: &str) -> Result<SynthesisResult> {
        let start = std::time::Instant::now();

        let result = match self.config.provider {
            TtsProvider::OpenAiTts => self.synthesize_openai(text).await?,
            TtsProvider::AzureTts => self.synthesize_azure(text).await?,
            TtsProvider::GoogleTts => self.synthesize_google(text).await?,
            TtsProvider::ElevenLabs => self.synthesize_elevenlabs(text).await?,
            TtsProvider::LocalTts => self.synthesize_local(text).await?,
            TtsProvider::Mock => self.synthesize_mock(text).await?,
        };

        let mut result = result;
        result.processing_time = start.elapsed();
        result.character_count = text.chars().count();

        tracing::debug!(
            "Synthesized {} characters in {:?}",
            result.character_count,
            result.processing_time
        );

        Ok(result)
    }

    /// Synthesize and play audio.
    pub async fn speak(&self, text: &str) -> Result<()> {
        let result = self.synthesize(text).await?;
        self.play_audio(&result.audio, result.format).await
    }

    /// Play audio data.
    pub async fn play_audio(&self, audio: &[u8], format: AudioFormat) -> Result<()> {
        // In a real implementation, use rodio to play audio
        // For now, just log
        tracing::debug!("Playing {} bytes of {:?} audio", audio.len(), format);

        // Simulate playback time based on file size (rough estimate)
        let estimated_duration = match format {
            AudioFormat::Mp3 => audio.len() as f64 / 16000.0, // ~128kbps
            AudioFormat::Wav => audio.len() as f64 / 88200.0, // 44.1kHz 16-bit stereo
            _ => audio.len() as f64 / 16000.0,
        };

        tokio::time::sleep(Duration::from_secs_f64(estimated_duration.min(0.1))).await;

        Ok(())
    }

    /// Play a feedback sound.
    pub async fn play_feedback(&self, feedback: FeedbackSound) -> Result<()> {
        // Generate simple feedback tones
        let (frequency, duration_ms) = match feedback {
            FeedbackSound::Ready => (440.0, 200),        // A4 note
            FeedbackSound::Success => (523.0, 150),      // C5 note
            FeedbackSound::Error => (220.0, 300),        // A3 note
            FeedbackSound::Processing => (330.0, 100),   // E4 note
            FeedbackSound::SessionStart => (440.0, 400), // A4 long
            FeedbackSound::SessionEnd => (262.0, 400),   // C4 long
        };

        // In real implementation, generate and play sine wave
        tracing::debug!(
            "Playing feedback {:?}: {}Hz for {}ms",
            feedback,
            frequency,
            duration_ms
        );

        tokio::time::sleep(Duration::from_millis(duration_ms as u64)).await;

        Ok(())
    }

    /// Synthesize using OpenAI TTS API.
    async fn synthesize_openai(&self, text: &str) -> Result<SynthesisResult> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| VoiceError::Config("OpenAI API key not configured".to_string()))?;

        let endpoint = self
            .config
            .endpoint
            .as_deref()
            .unwrap_or("https://api.openai.com/v1/audio/speech");

        let response_format = match self.config.format {
            AudioFormat::Mp3 => "mp3",
            AudioFormat::Wav => "wav",
            AudioFormat::Opus => "opus",
            AudioFormat::Aac => "aac",
            AudioFormat::Flac => "flac",
            AudioFormat::Pcm => "pcm",
        };

        let request_body = serde_json::json!({
            "model": "tts-1-hd",
            "input": text,
            "voice": self.config.voice.id,
            "response_format": response_format,
            "speed": self.config.rate
        });

        let response = self
            .client
            .post(endpoint)
            .bearer_auth(api_key)
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(VoiceError::Api(format!(
                "OpenAI TTS API error: {}",
                error_text
            )));
        }

        let audio = response.bytes().await?.to_vec();

        // Estimate duration (rough calculation for MP3)
        let estimated_duration = audio.len() as f32 / 16000.0;

        Ok(SynthesisResult {
            audio,
            format: self.config.format,
            duration: estimated_duration,
            sample_rate: 24000,
            processing_time: Duration::ZERO,
            character_count: 0,
        })
    }

    /// Synthesize using Azure TTS.
    async fn synthesize_azure(&self, text: &str) -> Result<SynthesisResult> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| VoiceError::Config("Azure API key not configured".to_string()))?;

        let endpoint = self.config.endpoint.as_deref().ok_or_else(|| {
            VoiceError::Config("Azure endpoint not configured".to_string())
        })?;

        let url = format!("{}/cognitiveservices/v1", endpoint);

        // Build SSML
        let ssml = format!(
            r#"<speak version='1.0' xml:lang='{}'>
                <voice name='{}'>
                    <prosody rate='{}' pitch='{}st'>
                        {}
                    </prosody>
                </voice>
            </speak>"#,
            self.config.voice.language,
            self.config.voice.id,
            format!("{:.0}%", (self.config.rate - 1.0) * 100.0),
            self.config.pitch,
            text
        );

        let response = self
            .client
            .post(&url)
            .header("Ocp-Apim-Subscription-Key", api_key)
            .header("Content-Type", "application/ssml+xml")
            .header(
                "X-Microsoft-OutputFormat",
                "audio-16khz-128kbitrate-mono-mp3",
            )
            .body(ssml)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(VoiceError::Api(format!(
                "Azure TTS API error: {}",
                error_text
            )));
        }

        let audio = response.bytes().await?.to_vec();
        let estimated_duration = audio.len() as f32 / 16000.0;

        Ok(SynthesisResult {
            audio,
            format: AudioFormat::Mp3,
            duration: estimated_duration,
            sample_rate: 16000,
            processing_time: Duration::ZERO,
            character_count: 0,
        })
    }

    /// Synthesize using Google Cloud TTS.
    async fn synthesize_google(&self, text: &str) -> Result<SynthesisResult> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| VoiceError::Config("Google API key not configured".to_string()))?;

        let url = format!(
            "https://texttospeech.googleapis.com/v1/text:synthesize?key={}",
            api_key
        );

        let request_body = serde_json::json!({
            "input": {
                "text": text
            },
            "voice": {
                "languageCode": self.config.voice.language,
                "name": self.config.voice.id,
            },
            "audioConfig": {
                "audioEncoding": "MP3",
                "speakingRate": self.config.rate,
                "pitch": self.config.pitch,
                "volumeGainDb": (self.config.volume - 1.0) * 6.0
            }
        });

        let response = self.client.post(&url).json(&request_body).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(VoiceError::Api(format!(
                "Google TTS API error: {}",
                error_text
            )));
        }

        let response_json: GoogleTtsResponse = response
            .json()
            .await
            .map_err(|e| VoiceError::Api(e.to_string()))?;

        let audio = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &response_json.audio_content,
        )
        .map_err(|e| VoiceError::Synthesis(format!("Failed to decode audio: {}", e)))?;

        let estimated_duration = audio.len() as f32 / 16000.0;

        Ok(SynthesisResult {
            audio,
            format: AudioFormat::Mp3,
            duration: estimated_duration,
            sample_rate: 24000,
            processing_time: Duration::ZERO,
            character_count: 0,
        })
    }

    /// Synthesize using ElevenLabs.
    async fn synthesize_elevenlabs(&self, text: &str) -> Result<SynthesisResult> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| VoiceError::Config("ElevenLabs API key not configured".to_string()))?;

        let url = format!(
            "https://api.elevenlabs.io/v1/text-to-speech/{}",
            self.config.voice.id
        );

        let request_body = serde_json::json!({
            "text": text,
            "model_id": "eleven_multilingual_v2",
            "voice_settings": {
                "stability": 0.5,
                "similarity_boost": 0.75,
                "style": 0.0,
                "use_speaker_boost": true
            }
        });

        let response = self
            .client
            .post(&url)
            .header("xi-api-key", api_key)
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(VoiceError::Api(format!(
                "ElevenLabs API error: {}",
                error_text
            )));
        }

        let audio = response.bytes().await?.to_vec();
        let estimated_duration = audio.len() as f32 / 16000.0;

        Ok(SynthesisResult {
            audio,
            format: AudioFormat::Mp3,
            duration: estimated_duration,
            sample_rate: 44100,
            processing_time: Duration::ZERO,
            character_count: 0,
        })
    }

    /// Synthesize using local TTS.
    async fn synthesize_local(&self, text: &str) -> Result<SynthesisResult> {
        // In real implementation, use espeak or piper
        tracing::warn!("Local TTS not implemented, using mock");
        self.synthesize_mock(text).await
    }

    /// Mock synthesis for testing.
    async fn synthesize_mock(&self, text: &str) -> Result<SynthesisResult> {
        // Simulate processing delay
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Generate silence WAV
        let sample_rate = 16000u32;
        let duration_secs = text.len() as f32 * 0.05; // ~50ms per character
        let num_samples = (sample_rate as f32 * duration_secs) as usize;

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut buffer = std::io::Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(&mut buffer, spec)
                .map_err(|e| VoiceError::Synthesis(e.to_string()))?;

            for _ in 0..num_samples {
                writer
                    .write_sample(0i16)
                    .map_err(|e| VoiceError::Synthesis(e.to_string()))?;
            }
            writer
                .finalize()
                .map_err(|e| VoiceError::Synthesis(e.to_string()))?;
        }

        Ok(SynthesisResult {
            audio: buffer.into_inner(),
            format: AudioFormat::Wav,
            duration: duration_secs,
            sample_rate,
            processing_time: Duration::from_millis(50),
            character_count: text.chars().count(),
        })
    }

    /// Get available voices for current provider.
    pub async fn list_voices(&self) -> Result<Vec<Voice>> {
        match self.config.provider {
            TtsProvider::OpenAiTts => Ok(vec![
                Voice {
                    id: "alloy".to_string(),
                    name: "Alloy".to_string(),
                    language: "en".to_string(),
                    gender: VoiceGender::Neutral,
                    style: None,
                },
                Voice {
                    id: "echo".to_string(),
                    name: "Echo".to_string(),
                    language: "en".to_string(),
                    gender: VoiceGender::Male,
                    style: None,
                },
                Voice {
                    id: "fable".to_string(),
                    name: "Fable".to_string(),
                    language: "en".to_string(),
                    gender: VoiceGender::Neutral,
                    style: None,
                },
                Voice {
                    id: "onyx".to_string(),
                    name: "Onyx".to_string(),
                    language: "en".to_string(),
                    gender: VoiceGender::Male,
                    style: None,
                },
                Voice {
                    id: "nova".to_string(),
                    name: "Nova".to_string(),
                    language: "en".to_string(),
                    gender: VoiceGender::Female,
                    style: None,
                },
                Voice {
                    id: "shimmer".to_string(),
                    name: "Shimmer".to_string(),
                    language: "en".to_string(),
                    gender: VoiceGender::Female,
                    style: None,
                },
            ]),
            _ => Ok(vec![Voice::default()]),
        }
    }

    /// Get the current configuration.
    pub fn config(&self) -> &TtsConfig {
        &self.config
    }

    /// Update the configuration.
    pub fn set_config(&mut self, config: TtsConfig) {
        self.config = config;
    }

    /// Set the voice.
    pub fn set_voice(&mut self, voice: Voice) {
        self.config.voice = voice;
    }

    /// Set the speech rate.
    pub fn set_rate(&mut self, rate: f32) {
        self.config.rate = rate.clamp(0.25, 4.0);
    }

    /// Set the volume.
    pub fn set_volume(&mut self, volume: f32) {
        self.config.volume = volume.clamp(0.0, 1.0);
    }
}

// Google TTS response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleTtsResponse {
    audio_content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tts_config_default() {
        let config = TtsConfig::default();
        assert_eq!(config.provider, TtsProvider::OpenAiTts);
        assert_eq!(config.rate, 1.0);
        assert_eq!(config.volume, 1.0);
    }

    #[test]
    fn test_voice_default() {
        let voice = Voice::default();
        assert_eq!(voice.id, "alloy");
        assert_eq!(voice.language, "en");
    }

    #[test]
    fn test_tts_provider_variants() {
        assert_eq!(TtsProvider::default(), TtsProvider::OpenAiTts);

        let providers = [
            TtsProvider::OpenAiTts,
            TtsProvider::AzureTts,
            TtsProvider::GoogleTts,
            TtsProvider::ElevenLabs,
            TtsProvider::LocalTts,
            TtsProvider::Mock,
        ];

        assert_eq!(providers.len(), 6);
    }

    #[tokio::test]
    async fn test_mock_synthesis() {
        let config = TtsConfig {
            provider: TtsProvider::Mock,
            ..Default::default()
        };

        let synthesizer = Synthesizer::new(config).await.unwrap();

        let result = synthesizer.synthesize("Hello, world!").await.unwrap();

        assert!(!result.audio.is_empty());
        assert!(result.duration > 0.0);
        assert_eq!(result.format, AudioFormat::Wav);
    }

    #[tokio::test]
    async fn test_list_voices() {
        let config = TtsConfig {
            provider: TtsProvider::OpenAiTts,
            ..Default::default()
        };

        let synthesizer = Synthesizer::new(config).await.unwrap();
        let voices = synthesizer.list_voices().await.unwrap();

        assert!(!voices.is_empty());
        assert!(voices.iter().any(|v| v.id == "alloy"));
    }

    #[test]
    fn test_audio_format_variants() {
        assert_eq!(AudioFormat::default(), AudioFormat::Mp3);

        let formats = [
            AudioFormat::Mp3,
            AudioFormat::Wav,
            AudioFormat::Opus,
            AudioFormat::Aac,
            AudioFormat::Flac,
            AudioFormat::Pcm,
        ];

        assert_eq!(formats.len(), 6);
    }
}