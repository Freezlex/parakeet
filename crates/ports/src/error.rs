use core::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    Unreachable(String),
    Rejected(String),
    Other(String),
}

impl TransportError {
    pub fn is_recoverable(&self) -> bool {
        matches!(self, TransportError::Unreachable(_))
    }

    pub fn unreachable(reason: impl Into<String>) -> Self {
        TransportError::Unreachable(reason.into())
    }

    pub fn rejected(reason: impl Into<String>) -> Self {
        TransportError::Rejected(reason.into())
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::Unreachable(why) => write!(f, "unreachable: {why}"),
            TransportError::Rejected(why) => write!(f, "rejected: {why}"),
            TransportError::Other(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for TransportError {}
