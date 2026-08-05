#![forbid(unsafe_code)]

//! Native presentation and interaction for one authoritative Orbit session.

mod accessibility;
mod input;
mod model;
mod render;
mod scene;
mod transport;

pub use accessibility::{Accessibility, Activation};
pub use input::InputState;
pub use model::{ConnectionState, ModelError, SessionModel};
pub use render::{CellMetrics, PresentOutcome, RenderError, Renderer};
pub use scene::{Color, DrawCell, DrawCursor, DrawRow, DrawStyle, GlyphRun, Scene};
pub use transport::{SendError, Transport, TransportEvent};
