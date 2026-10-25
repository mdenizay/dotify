use dotify_core::format::FormatError;
use dotify_core::CoreError;
use dotify_export::ExportError;
use serde_json::{json, Value};

/// A tool failure the caller can read and act on. `code` is stable and
/// machine-readable; engine errors keep the code of `CoreError::code()`.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolError {
    pub code: &'static str,
    pub message: String,
    pub details: Option<Value>,
}

impl ToolError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), details: None }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid_arguments", message)
    }

    pub fn confirmation_required(message: impl Into<String>) -> Self {
        Self::new("confirmation_required", message)
    }

    pub fn to_json(&self) -> Value {
        let mut error = json!({ "code": self.code, "message": self.message });
        if let Some(d) = &self.details {
            error["details"] = d.clone();
        }
        json!({ "error": error })
    }
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for ToolError {}

impl From<CoreError> for ToolError {
    fn from(e: CoreError) -> Self {
        let details = match &e {
            CoreError::RevisionMismatch { expected, actual } => Some(json!({ "expected": expected, "actual": actual })),
            _ => None,
        };
        Self { code: e.code(), message: e.to_string(), details }
    }
}

impl From<FormatError> for ToolError {
    fn from(e: FormatError) -> Self {
        Self::new(e.code(), e.to_string())
    }
}

impl From<ExportError> for ToolError {
    fn from(e: ExportError) -> Self {
        Self::new(e.code(), e.to_string())
    }
}

impl From<std::io::Error> for ToolError {
    fn from(e: std::io::Error) -> Self {
        Self::new("io_error", e.to_string())
    }
}

pub type ToolResult<T> = Result<T, ToolError>;
