//! Authentication mail: payload encryption and bounded SMTP submission.

pub mod crypto;
pub mod transport;

/// The safe, fixed vocabulary for why a mail operation failed. Nothing about
/// the relay's response, the message or its recipient is carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FailureCategory {
    /// The relay could not be reached or the session broke.
    #[error("connection")]
    Connection,
    /// TLS could not be established or the certificate was refused.
    #[error("tls")]
    Tls,
    /// The relay rejected the configured login.
    #[error("authentication")]
    Authentication,
    /// The relay deferred the message (4xx); a later attempt may succeed.
    #[error("temporary_refusal")]
    TemporaryRefusal,
    /// The relay rejected the message (5xx); retrying cannot help.
    #[error("permanent_refusal")]
    PermanentRefusal,
    /// The whole exchange exceeded its time bound.
    #[error("timeout")]
    Timeout,
    /// The stored payload failed authentication or could not be turned into a message.
    #[error("payload_unreadable")]
    PayloadUnreadable,
}

impl FailureCategory {
    /// The stable identifier used in logs and job status.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Connection => "connection",
            Self::Tls => "tls",
            Self::Authentication => "authentication",
            Self::TemporaryRefusal => "temporary_refusal",
            Self::PermanentRefusal => "permanent_refusal",
            Self::Timeout => "timeout",
            Self::PayloadUnreadable => "payload_unreadable",
        }
    }
}
