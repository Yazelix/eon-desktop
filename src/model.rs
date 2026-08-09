use crate::scene::Scene;
use eon_workspace_protocol::{Response as WorkspaceResponse, Snapshot};
use orbit_protocol::FrameReducer;
use orbit_protocol::session::{FailureCode, ServerMessage};
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

#[derive(Debug)]
enum Notice {
    Orbit(String),
    Venus(LocalNoticeSource, String),
}

/// Venus path allowed to resolve its own client-visible notice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalNoticeSource {
    Input,
    Queue,
    Resize,
    Clipboard,
}

/// The sole persistent owner of accepted presentation state in one client process.
#[derive(Debug)]
pub struct SessionModel {
    reducer: FrameReducer,
    scene: Option<Scene>,
    connection: ConnectionState,
    notices: Vec<Notice>,
}

/// The last complete Eon-authored workspace accepted by Venus.
#[derive(Debug, Default)]
pub struct WorkspaceModel {
    snapshot: Option<Snapshot>,
    notice: Option<String>,
}

impl WorkspaceModel {
    #[must_use]
    pub fn snapshot(&self) -> Option<&Snapshot> {
        self.snapshot.as_ref()
    }

    #[must_use]
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// Replace workspace state only with a complete accepted snapshot.
    pub fn apply(&mut self, response: WorkspaceResponse) -> bool {
        match response {
            WorkspaceResponse::Snapshot(snapshot) => {
                let changed = self.snapshot.as_ref() != Some(&snapshot);
                self.snapshot = Some(snapshot);
                self.notice = None;
                changed
            }
            WorkspaceResponse::Failure(failure) => {
                self.notice = Some(bounded(format!(
                    "Eon workspace {}: {}",
                    failure.code, failure.detail
                )));
                false
            }
        }
    }

    pub fn mark_unavailable(&mut self, detail: impl Into<String>) {
        self.notice = Some(bounded(detail.into()));
    }

    #[must_use]
    pub fn active_endpoint(&self) -> Option<&[u8]> {
        let snapshot = self.snapshot.as_ref()?;
        let tab = snapshot
            .tabs
            .iter()
            .find(|tab| tab.id == snapshot.active_tab)?;
        tab.panes
            .iter()
            .find(|pane| pane.id == tab.selected_pane)
            .map(|pane| pane.endpoint.as_slice())
    }
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
            notices: Vec::with_capacity(4),
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
        self.notices.last().map(|notice| match notice {
            Notice::Orbit(detail) | Notice::Venus(_, detail) => detail.as_str(),
        })
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

    pub fn apply(&mut self, message: ServerMessage) -> Result<Option<String>, ModelError> {
        let in_order = match &message {
            ServerMessage::Attached { .. }
            | ServerMessage::Busy
            | ServerMessage::Incompatible { .. } => {
                matches!(self.connection, ConnectionState::Connecting)
            }
            ServerMessage::Frame(_)
            | ServerMessage::Accepted
            | ServerMessage::CopiedText(_)
            | ServerMessage::Exited { .. } => self.is_attached(),
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
                self.notices.clear();
            }
            ServerMessage::Frame(frame) => {
                let frame = self.reducer.push(*frame).map_err(ModelError::Frame)?;
                self.scene = Some(Scene::from_frame(frame));
            }
            ServerMessage::Accepted => {
                self.clear_orbit_notice();
            }
            ServerMessage::Failure(failure) => {
                let label = match failure.code {
                    FailureCode::InvalidInput => "rejected input",
                    FailureCode::Protocol => "protocol failure",
                    FailureCode::Terminal => "terminal failure",
                };
                self.clear_orbit_notice();
                self.notices.push(Notice::Orbit(bounded(format!(
                    "Orbit {label}: {}",
                    failure.detail
                ))));
            }
            ServerMessage::Busy => {
                self.connection = ConnectionState::Busy;
                self.notices.clear();
            }
            ServerMessage::Incompatible {
                minimum_version,
                maximum_version,
            } => {
                self.connection = ConnectionState::Incompatible {
                    minimum: minimum_version,
                    maximum: maximum_version,
                };
                self.notices.clear();
            }
            ServerMessage::Exited { code } => {
                self.connection = ConnectionState::Exited { code };
                self.notices.clear();
            }
            ServerMessage::CopiedText(text) => {
                self.clear_orbit_notice();
                return Ok(Some(text));
            }
        }
        Ok(None)
    }

    pub fn mark_lost(&mut self, detail: impl Into<String>) {
        if self.is_terminal() {
            return;
        }
        self.connection = ConnectionState::Lost {
            detail: bounded(detail.into()),
        };
        self.notices.clear();
    }

    pub fn prepare_reconnect(&mut self) {
        self.reducer = FrameReducer::default();
        self.connection = ConnectionState::Connecting;
        self.notices.clear();
    }

    pub fn set_venus_notice(&mut self, source: LocalNoticeSource, detail: impl Into<String>) {
        if self.is_terminal() {
            return;
        }
        self.clear_venus_notice(source);
        self.notices
            .push(Notice::Venus(source, bounded(detail.into())));
    }

    pub fn clear_venus_notice(&mut self, source: LocalNoticeSource) -> bool {
        let previous_len = self.notices.len();
        self.notices
            .retain(|notice| !matches!(notice, Notice::Venus(owner, _) if *owner == source));
        self.notices.len() != previous_len
    }

    fn clear_orbit_notice(&mut self) {
        self.notices
            .retain(|notice| !matches!(notice, Notice::Orbit(_)));
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

#[cfg(test)]
mod tests {
    use super::*;
    use eon_workspace_protocol::{Failure, Pane, Tab};

    #[test]
    fn rejected_workspace_action_preserves_the_last_complete_snapshot() {
        let snapshot = Snapshot {
            active_tab: "tab-1".into(),
            tabs: vec![Tab {
                id: "tab-1".into(),
                selected_pane: "pane-1".into(),
                panes: vec![Pane {
                    id: "pane-1".into(),
                    session: "session-1".into(),
                    endpoint: b"/run/eon/orbit.sock".to_vec(),
                    live: true,
                }],
            }],
        };
        let mut model = WorkspaceModel::default();

        assert!(model.apply(WorkspaceResponse::Snapshot(snapshot.clone())));
        assert!(!model.apply(WorkspaceResponse::Failure(Failure {
            code: "edge".into(),
            detail: "there is no pane above the selected pane".into(),
        })));

        assert_eq!(model.snapshot(), Some(&snapshot));
        assert_eq!(model.active_endpoint(), Some(&b"/run/eon/orbit.sock"[..]));
        assert_eq!(
            model.notice(),
            Some("Eon workspace edge: there is no pane above the selected pane")
        );
    }
}
