//! XML-backed protocol compilation and binary codecs.
use serde::{Deserialize, Serialize};
mod codec;
mod expression;
mod schema;
pub use codec::*;
pub use schema::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
    pub path: Option<String>,
    pub offset: Option<usize>,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for ProtocolError {}
impl ProtocolError {
    pub(crate) fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            path: None,
            offset: None,
            line: None,
            column: None,
        }
    }
    pub(crate) fn at(mut self, path: &str, offset: usize) -> Self {
        self.path = Some(path.into());
        self.offset = Some(offset);
        self
    }
}
