use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    #[error("Invalid message type: 0x{0:02X} ('{}')", *.0 as char)]
    InvalidMessageType(u8),

    #[error("Truncated message: need {expected} bytes, have {actual}")]
    TruncatedMessage { expected: usize, actual: usize },

    #[error("Invalid header: {reason}")]
    InvalidHeader { reason: &'static str },

    #[error("Message length mismatch: declared {declared}, expected {expected} for type 0x{msg_type:02X}")]
    LengthMismatch {
        msg_type: u8,
        declared: usize,
        expected: usize,
    },

    #[error("Buffer overflow: input size {size} exceeds maximum {max}")]
    BufferOverflow { size: usize, max: usize },

    #[error("Missing field '{field}' at offset {offset}")]
    MissingField { field: &'static str, offset: usize },

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Invalid UTF-8 in field '{field}'")]
    InvalidUtf8 { field: &'static str },

    #[error("Parser state error: {reason}")]
    StateError { reason: &'static str },

    #[error("IO error: {0}")]
    Io(String),
}

impl From<std::io::Error> for ParseError {
    fn from(err: std::io::Error) -> Self {
        ParseError::Io(err.to_string())
    }
}

impl ParseError {
    #[inline]
    pub fn is_truncated(&self) -> bool {
        matches!(self, ParseError::TruncatedMessage { .. })
    }

    #[inline]
    pub fn is_corrupt(&self) -> bool {
        matches!(
            self,
            ParseError::InvalidMessageType(_)
                | ParseError::InvalidHeader { .. }
                | ParseError::LengthMismatch { .. }
                | ParseError::InvalidUtf8 { .. }
        )
    }

    #[inline]
    pub fn is_limit(&self) -> bool {
        matches!(self, ParseError::BufferOverflow { .. })
    }
}

pub type Result<T> = std::result::Result<T, ParseError>;

pub type ParseResult<T> = Result<Option<T>>;
