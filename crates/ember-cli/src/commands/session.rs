//! Session persistence for chat conversations.
//!
//! Handles saving, loading, and managing chat sessions stored as JSON
//! in `~/.ember/sessions/`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use ember_llm::Message;

/// A persisted chat session stored as JSON in `~/.ember/sessions/`.
#[derive(Debug, Serialize, Deserialize)]
pub struct PersistedSession {
    /// Unique session identifier (UUIDv4-style hex string).
    pub id: String,
    /// Provider name used for this session.
    pub provider: String,
    /// Model name used for this session.
    pub model: String,
    /// ISO-8601 timestamp when the session was created.
    pub created_at: String,
    /// ISO-8601 timestamp of the last message.
    pub updated_at: String,
    /// Message history (serialised form of `ember_llm::Message`).
    pub messages: Vec<PersistedMessage>,
    /// Total turn count (system message excluded).
    pub turn_count: usize,
    /// Active model override (may differ from `model` after /model switch).
    #[serde(default)]
    pub active_model: Option<String>,
    /// Whether compact mode was active.
    #[serde(default)]
    pub compact_mode: bool,
    /// Whether plan mode was active.
    #[serde(default)]
    pub plan_mode: bool,
    /// Working directory at time of save.
    #[serde(default)]
    pub working_directory: Option<String>,
    /// Cumulative cost in USD at time of save.
    #[serde(default)]
    pub total_cost_usd: f64,
}

/// A single serialised message in a persisted session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedMessage {
    /// Role: "system" | "user" | "assistant" | "tool"
    pub role: String,
    /// Message text content.
    pub content: String,
    /// Tool-call id (only set for tool-result messages).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl PersistedMessage {
    pub fn from_message(msg: &Message) -> Self {
        let role = format!("{:?}", msg.role).to_lowercase();
        Self {
            role,
            content: msg.content.clone(),
            tool_call_id: msg.tool_call_id.clone(),
        }
    }

    pub fn to_message(&self) -> Message {
        match self.role.as_str() {
            "system" => Message::system(&self.content),
            "user" => Message::user(&self.content),
            "assistant" => Message::assistant(&self.content),
            "tool" => {
                let id = self.tool_call_id.as_deref().unwrap_or("unknown");
                Message::tool_result(id, &self.content)
            }
            _ => Message::user(&self.content),
        }
    }
}

/// Return the path to the sessions directory, creating it if needed.
pub fn sessions_dir() -> Result<std::path::PathBuf> {
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    let dir = home.join(".ember").join("sessions");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Generate a short random hex session id.
pub fn new_session_id() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    // Use timestamp + nanos + pid for uniqueness (pseudo-UUID)
    let secs = now.as_secs();
    let nanos = now.subsec_nanos();
    let pid = std::process::id();
    format!("{:08x}-{:08x}-{:04x}", secs, nanos, pid & 0xFFFF)
}

/// Save a session to disk.
pub fn save_session(session: &PersistedSession) -> Result<()> {
    let dir = sessions_dir()?;
    let path = dir.join(format!("{}.json", session.id));
    let json = serde_json::to_string_pretty(session)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Load a session by id.
pub fn load_session(id: &str) -> Result<PersistedSession> {
    let dir = sessions_dir()?;
    let path = dir.join(format!("{}.json", id));
    let json =
        std::fs::read_to_string(&path).with_context(|| format!("Session '{}' not found", id))?;
    let session: PersistedSession = serde_json::from_str(&json)?;
    Ok(session)
}

/// Find the most recently modified session file and return its id.
pub fn latest_session_id() -> Option<String> {
    let dir = sessions_dir().ok()?;
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();

    entries.sort_by_key(|e| {
        e.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });

    entries
        .last()
        .and_then(|e| e.path().file_stem()?.to_str().map(str::to_owned))
}

/// Current time as a simple ISO-8601 string (without external dependencies).
pub fn now_iso8601() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Produce a compact UTC timestamp: YYYY-MM-DDTHH:MM:SSZ
    let s = secs;
    let (y, mo, d, h, mi, sec) = seconds_to_ymd_hms(s);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, mo, d, h, mi, sec)
}

pub fn seconds_to_ymd_hms(mut s: u64) -> (u32, u32, u32, u32, u32, u32) {
    let sec = (s % 60) as u32;
    s /= 60;
    let min = (s % 60) as u32;
    s /= 60;
    let hour = (s % 24) as u32;
    s /= 24;
    // Days since 1970-01-01 → Gregorian date (simplified, good until ~2100)
    let mut days = s as u32;
    let mut y = 1970u32;
    loop {
        let dy = if (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400) {
            366
        } else {
            365
        };
        if days < dy {
            break;
        }
        days -= dy;
        y += 1;
    }
    let leap = (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400);
    let month_days = [
        31u32,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut mo = 0u32;
    for md in &month_days {
        if days < *md {
            break;
        }
        days -= md;
        mo += 1;
    }
    (y, mo + 1, days + 1, hour, min, sec)
}
