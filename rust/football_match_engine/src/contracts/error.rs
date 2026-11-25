use thiserror::Error;

#[derive(Debug, Error)]
pub enum V4Error {
    #[error("invalid protocol version: expected {expected}, got {actual}")]
    InvalidProtocolVersion { expected: i32, actual: i32 },

    #[error("invalid attribute '{field}': value {value} is outside 1..=20")]
    InvalidAttribute { field: &'static str, value: u8 },

    #[error("invalid match input: {0}")]
    InvalidInput(String),

    #[error("invalid team roster '{team_id}': {reason}")]
    InvalidRoster { team_id: String, reason: String },

    #[error("match clock error: {0}")]
    InvalidClock(String),

    #[error("20-minute compressed mode is not supported in Match Engine V4; regulation is always 90 minutes")]
    UnsupportedCompressionMode,

    #[error("session already finished")]
    SessionFinished,

    #[error("json serialization/deserialization error: {0}")]
    Json(#[from] serde_json::Error),
}
