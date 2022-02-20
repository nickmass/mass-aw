#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    InvalidMemEntryState(u8),
    InvalidBankId(u8),
    CrcCheckFailed,
    InputBufferDrained,
}

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(err) => write!(f, "{}", err),
            Error::InvalidMemEntryState(value) => write!(f, "invalid mem entry state: {}", value),
            _ => write!(f, "unknown error"),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(inner: std::io::Error) -> Self {
        Error::Io(inner)
    }
}
