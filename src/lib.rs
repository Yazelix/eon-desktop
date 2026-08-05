#![forbid(unsafe_code)]

//! Native presentation and interaction for one authoritative Orbit session.

pub mod accessibility;
pub mod input;
pub mod model;
pub mod render;
pub mod scene;
pub mod transport;

pub use accessibility::{Accessibility, Activation};
pub use input::{InputState, physical_key};
pub use model::{ConnectionState, ModelError, SessionModel};
pub use render::{CellMetrics, PresentOutcome, Renderer};
pub use scene::{Color, DrawCell, DrawCursor, DrawRow, DrawStyle, GlyphRun, Scene};
pub use transport::{SendError, Transport, TransportEvent};
