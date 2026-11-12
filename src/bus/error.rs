use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusError {
    Refused(String),
    Failed(String),
}

impl BusError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Refused(_) => 2,
            Self::Failed(_) => 4,
        }
    }
}
impl fmt::Display for BusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(s) | Self::Failed(s) => f.write_str(s),
        }
    }
}
impl std::error::Error for BusError {}
impl From<rusqlite::Error> for BusError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Failed(format!("bus database: {e}"))
    }
}
impl From<std::io::Error> for BusError {
    fn from(e: std::io::Error) -> Self {
        Self::Failed(format!("bus I/O: {e}"))
    }
}
impl From<serde_json::Error> for BusError {
    fn from(e: serde_json::Error) -> Self {
        Self::Failed(format!("bus JSON: {e}"))
    }
}
