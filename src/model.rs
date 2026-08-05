use crate::scene::Scene;
use orbit_protocol::{FrameReducer, session::ServerMessage};
use std::{error::Error, fmt};

/// Bounded lifecycle state for one local Orbit attachment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionState {
    Connecting,
    Attached { version: u16 },
    Busy,
    Incompatible { minimum: u16, maximum: u16 },
    Lost { detail: String },
    Exited { code: i32 },
}

/// Observable result of applying one server message.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModelChange {
    pub scene: bool,
    pub status: bool,
}

/// A message violated the accepted local-session order or frame sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelError {
    MessageBeforeAttachment,
    DuplicateAttachment,
    Frame(orbit_protocol::Error),
}

impl fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MessageBeforeAttachment => {
                formatter.write_str("Orbit sent presentation state before attachment")
            }
            Self::DuplicateAttachment => formatter.write_str("Orbit attached the session twice"),
            Self::Frame(error) => write!(formatter, "Orbit frame rejected: {error}"),
        }
    }
}

impl Error for ModelError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Frame(error) => Some(error),
            _ => None,
        }
    }
}

/// The sole persistent owner of accepted presentation state in one client process.
#[derive(Debug)]
pub struct SessionModel {
    reducer: FrameReducer,
    scene: Option<Scene>,
    connection: ConnectionState,
    notice: Option<String>,
}

impl Default for SessionModel {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionModel {
    #[must_use]
    pub fn new() -> Self {
        Self {
            reducer: FrameReducer::default(),
            scene: None,
            connection: ConnectionState::Connecting,
            notice: None,
        }
    }

    #[must_use]
    pub fn scene(&self) -> Option<&Scene> {
        self.scene.as_ref()
    }

    #[must_use]
    pub fn connection(&self) -> &ConnectionState {
        &self.connection
    }

    #[must_use]
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    #[must_use]
    pub fn is_attached(&self) -> bool {
        matches!(self.connection, ConnectionState::Attached { .. })
    }

    pub fn apply(&mut self, message: ServerMessage) -> Result<ModelChange, ModelError> {
        match message {
            ServerMessage::Attached { version } => {
                if !matches!(self.connection, ConnectionState::Connecting) {
                    return Err(ModelError::DuplicateAttachment);
                }
                self.connection = ConnectionState::Attached { version };
                self.notice = None;
                Ok(ModelChange {
                    scene: false,
                    status: true,
                })
            }
            ServerMessage::Frame(frame) => {
                if !self.is_attached() {
                    return Err(ModelError::MessageBeforeAttachment);
                }
                let frame = self.reducer.push(*frame).map_err(ModelError::Frame)?;
                self.scene = Some(Scene::from_frame(frame));
                self.notice = None;
                Ok(ModelChange {
                    scene: true,
                    status: true,
                })
            }
            ServerMessage::Accepted => {
                let changed = self.notice.take().is_some();
                Ok(ModelChange {
                    scene: false,
                    status: changed,
                })
            }
            ServerMessage::Failure(failure) => {
                self.notice = Some(bounded(failure.detail));
                Ok(ModelChange {
                    scene: false,
                    status: true,
                })
            }
            ServerMessage::Busy => {
                self.connection = ConnectionState::Busy;
                self.notice = None;
                Ok(status_change())
            }
            ServerMessage::Incompatible {
                minimum_version,
                maximum_version,
            } => {
                self.connection = ConnectionState::Incompatible {
                    minimum: minimum_version,
                    maximum: maximum_version,
                };
                self.notice = None;
                Ok(status_change())
            }
            ServerMessage::Exited { code } => {
                self.connection = ConnectionState::Exited { code };
                self.notice = None;
                Ok(status_change())
            }
        }
    }

    pub fn mark_lost(&mut self, detail: impl Into<String>) -> ModelChange {
        self.connection = ConnectionState::Lost {
            detail: bounded(detail.into()),
        };
        self.notice = None;
        status_change()
    }

    pub fn set_notice(&mut self, detail: impl Into<String>) -> ModelChange {
        self.notice = Some(bounded(detail.into()));
        status_change()
    }
}

fn status_change() -> ModelChange {
    ModelChange {
        scene: false,
        status: true,
    }
}

fn bounded(mut detail: String) -> String {
    const MAX_CHARS: usize = 1024;
    if let Some((index, _)) = detail.char_indices().nth(MAX_CHARS) {
        detail.truncate(index);
        detail.push('…');
    }
    detail
}
