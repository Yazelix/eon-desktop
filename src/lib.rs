#![forbid(unsafe_code)]

//! Native presentation and interaction for one authoritative Orbit session.

mod accessibility;
mod input;
mod model;
mod render;
mod scene;
mod transport;

pub use accessibility::{Accessibility, AccessibilityTarget, Activation};
pub use input::InputState;
pub use model::{
    ClipboardEffect, ConnectionState, LocalNoticeSource, ModelError, SessionModel, WorkspaceModel,
    directory_picker_visible,
};
pub use render::{CellMetrics, PresentOutcome, RenderError, Renderer};
pub use scene::{
    Color, DrawCell, DrawCursor, DrawRow, DrawStyle, GlyphRun, Hyperlink, PaneMetadata, Scene,
    ScenePreview, SceneRect, WorkspaceFocus, WorkspaceHit, WorkspacePane, WorkspaceScene,
    WorkspaceTab,
};
pub use transport::{
    MetadataEvent, MetadataTransport, SendError, Transport, TransportEvent, WorkspaceEvent,
    WorkspaceTransport,
};
