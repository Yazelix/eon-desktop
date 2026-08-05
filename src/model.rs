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

/// A message violated the accepted local-session order or frame sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelError {
    UnexpectedMessage,
    Frame(orbit_protocol::Error),
}

impl fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedMessage => {
                formatter.write_str("Orbit sent a local-session message out of order")
            }
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

    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.connection,
            ConnectionState::Busy
                | ConnectionState::Incompatible { .. }
                | ConnectionState::Lost { .. }
                | ConnectionState::Exited { .. }
        )
    }

    pub fn apply(&mut self, message: ServerMessage) -> Result<(), ModelError> {
        let in_order = match &message {
            ServerMessage::Attached { .. }
            | ServerMessage::Busy
            | ServerMessage::Incompatible { .. } => {
                matches!(self.connection, ConnectionState::Connecting)
            }
            ServerMessage::Frame(_) | ServerMessage::Accepted | ServerMessage::Exited { .. } => {
                self.is_attached()
            }
            ServerMessage::Failure(_) => matches!(
                self.connection,
                ConnectionState::Connecting | ConnectionState::Attached { .. }
            ),
        };
        if !in_order {
            return Err(ModelError::UnexpectedMessage);
        }

        match message {
            ServerMessage::Attached { version } => {
                self.connection = ConnectionState::Attached { version };
                self.notice = None;
                Ok(())
            }
            ServerMessage::Frame(frame) => {
                let frame = self.reducer.push(*frame).map_err(ModelError::Frame)?;
                self.scene = Some(Scene::from_frame(frame));
                self.notice = None;
                Ok(())
            }
            ServerMessage::Accepted => {
                self.notice = None;
                Ok(())
            }
            ServerMessage::Failure(failure) => {
                self.notice = Some(bounded(format!("Orbit rejected input: {}", failure.detail)));
                Ok(())
            }
            ServerMessage::Busy => {
                self.connection = ConnectionState::Busy;
                self.notice = None;
                Ok(())
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
                Ok(())
            }
            ServerMessage::Exited { code } => {
                self.connection = ConnectionState::Exited { code };
                self.notice = None;
                Ok(())
            }
        }
    }

    pub fn mark_lost(&mut self, detail: impl Into<String>) {
        if self.is_terminal() {
            return;
        }
        self.connection = ConnectionState::Lost {
            detail: bounded(detail.into()),
        };
        self.notice = None;
    }

    pub fn set_notice(&mut self, detail: impl Into<String>) {
        if self.is_terminal() {
            return;
        }
        self.notice = Some(bounded(detail.into()));
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
