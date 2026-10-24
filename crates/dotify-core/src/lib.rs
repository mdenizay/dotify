//! Dotify's document engine: model, commands, history, compositing and the
//! `.dotify` project format. Independent of any UI or transport.

pub mod color;
pub mod command;
pub mod composite;
pub mod editor;
pub mod error;
pub mod format;
pub mod geometry;
pub mod image;
pub mod model;
mod ops;
pub mod png_io;
pub mod raster;

pub use color::Rgba;
pub use command::{Command, Created, PixelWrite};
pub use editor::{DirtyRegion, DocumentSummary, Editor, Outcome};
pub use error::{CoreError, CoreResult};
pub use geometry::{Point, Rect};
pub use image::Image;
pub use model::{AnimationTag, Document, Frame, FrameId, Layer, LayerId, Palette, PlaybackDirection};

/// Largest supported canvas side, in pixels.
pub const MAX_CANVAS_SIZE: u32 = 4096;
