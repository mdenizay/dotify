use crate::model::{FrameId, LayerId};
use crate::project::{ArtboardId, FolderId};

/// Errors returned by document commands. Every variant has a stable
/// machine-readable [`code`](CoreError::code) for structured API responses.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoreError {
    #[error("layer {0} does not exist")]
    LayerNotFound(LayerId),
    #[error("frame {0} does not exist")]
    FrameNotFound(FrameId),
    #[error("layer {0} is locked")]
    LayerLocked(LayerId),
    #[error("a document must keep at least one layer")]
    LastLayer,
    #[error("a document must keep at least one frame")]
    LastFrame,
    #[error("artboard {0} does not exist")]
    ArtboardNotFound(ArtboardId),
    #[error("folder {0} does not exist")]
    FolderNotFound(FolderId),
    #[error("a project must keep at least one artboard")]
    LastArtboard,
    #[error("canvas size {width}x{height} is invalid (allowed 1..={max} per side)", max = crate::MAX_CANVAS_SIZE)]
    InvalidCanvasSize { width: u32, height: u32 },
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("revision mismatch: expected {expected}, document is at {actual}")]
    RevisionMismatch { expected: u64, actual: u64 },
    #[error("internal error: {0}")]
    Internal(String),
}

impl CoreError {
    pub fn code(&self) -> &'static str {
        match self {
            CoreError::LayerNotFound(_) => "layer_not_found",
            CoreError::FrameNotFound(_) => "frame_not_found",
            CoreError::LayerLocked(_) => "layer_locked",
            CoreError::LastLayer => "last_layer",
            CoreError::LastFrame => "last_frame",
            CoreError::ArtboardNotFound(_) => "artboard_not_found",
            CoreError::FolderNotFound(_) => "folder_not_found",
            CoreError::LastArtboard => "last_artboard",
            CoreError::InvalidCanvasSize { .. } => "invalid_canvas_size",
            CoreError::InvalidArgument(_) => "invalid_argument",
            CoreError::RevisionMismatch { .. } => "revision_mismatch",
            CoreError::Internal(_) => "internal",
        }
    }
}

pub type CoreResult<T> = Result<T, CoreError>;
