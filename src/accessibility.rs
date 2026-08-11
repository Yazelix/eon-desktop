use crate::{
    Scene, WorkspaceFocus, WorkspaceScene,
    scene::{AccessiblePosition, AccessibleRow, AccessibleSelection, AccessibleText},
};
use accesskit::{
    Action, ActivationHandler, Node, NodeId, Orientation, Rect, Role, TextDirection, TextPosition,
    TextSelection, Tree, TreeId, TreeUpdate,
};
use accesskit_winit::Adapter;
use std::sync::{Arc, Mutex, MutexGuard};
use winit::dpi::PhysicalSize;

const WINDOW: NodeId = NodeId(0);
const CONTENT: NodeId = NodeId(1);
const STATUS: NodeId = NodeId(2);
const TAB_LIST: NodeId = NodeId(3);
const PANE_PANEL: NodeId = NodeId(4);

#[derive(Clone, Debug)]
struct Snapshot {
    title: String,
    content: AccessibleText,
    status: String,
    size: PhysicalSize<u32>,
    terminal: bool,
    workspace: Option<WorkspaceScene>,
    workspace_focus: WorkspaceFocus,
}

impl Snapshot {
    fn tree(&self) -> TreeUpdate {
        let mut root = Node::new(Role::Window);
        root.set_label(self.title.as_str());
        root.set_bounds(bounds(self.size));
        let status_alert = self.terminal && !self.status.is_empty();
        root.set_children(if self.workspace.is_some() {
            let mut children = vec![TAB_LIST, PANE_PANEL];
            if status_alert {
                children.push(STATUS);
            }
            children
        } else if status_alert {
            vec![CONTENT, STATUS]
        } else {
            vec![CONTENT]
        });

        let mut content = Node::new(if self.terminal {
            Role::Terminal
        } else {
            Role::Alert
        });
        content.set_label(if self.terminal {
            self.title.as_str()
        } else {
            "Venus status"
        });
        if self.terminal {
            content.set_read_only();
            content.set_children(
                (0..self.content.rows.len())
                    .map(text_run_id)
                    .collect::<Vec<_>>(),
            );
            if let Some(selection) = self.content.selection {
                content.set_text_selection(text_selection(selection));
            }
        } else {
            content.set_value(self.status.as_str());
        }
        let terminal_layout_bounds = self
            .workspace
            .as_ref()
            .map_or_else(|| bounds(self.size), |workspace| rect(workspace.terminal));
        let terminal_bounds = self
            .workspace
            .as_ref()
            .map_or(terminal_layout_bounds, |workspace| {
                rect(workspace.visible_terminal().unwrap_or(workspace.terminal))
            });
        content.set_bounds(terminal_bounds);

        let mut nodes = vec![(WINDOW, root), (CONTENT, content)];
        if let Some(workspace) = &self.workspace {
            let mut tab_list = Node::new(Role::TabList);
            tab_list.set_label("Eon workspace tabs");
            tab_list.set_orientation(Orientation::Horizontal);
            tab_list.set_bounds(rect(workspace.tab_viewport));
            tab_list.set_children((0..workspace.tabs.len()).map(tab_id).collect::<Vec<_>>());
            nodes.push((TAB_LIST, tab_list));
            for (index, tab) in workspace.tabs.iter().enumerate() {
                let mut node = Node::new(Role::Tab);
                node.set_label(tab.id.as_str());
                node.set_selected(tab.selected);
                node.set_bounds(rect(tab.rect));
                node.add_action(Action::Click);
                node.add_action(Action::Focus);
                nodes.push((tab_id(index), node));
            }

            let mut panel = Node::new(Role::TabPanel);
            panel.set_label("Active tab panes");
            panel.set_orientation(Orientation::Vertical);
            panel.set_bounds(rect(workspace.pane_viewport));
            let mut children = Vec::with_capacity(workspace.panes.len() + 1);
            for (index, pane) in workspace.panes.iter().enumerate() {
                children.push(pane_id(index));
                if pane.selected {
                    children.push(CONTENT);
                }
            }
            panel.set_children(children);
            nodes.push((PANE_PANEL, panel));
            for (index, pane) in workspace.panes.iter().enumerate() {
                let mut node = Node::new(Role::Button);
                node.set_label(pane.label());
                node.set_selected(pane.selected);
                node.set_expanded(pane.selected);
                node.set_bounds(rect(pane.rect));
                node.add_action(Action::Click);
                node.add_action(Action::Focus);
                nodes.push((pane_id(index), node));
            }
        }
        if self.terminal {
            for (index, row) in self.content.rows.iter().enumerate() {
                nodes.push((
                    text_run_id(index),
                    text_run(row, terminal_layout_bounds, index, self.content.rows.len()),
                ));
            }
        }
        if self.terminal && !self.status.is_empty() {
            let mut status = Node::new(Role::Alert);
            status.set_label("Venus status");
            status.set_value(self.status.as_str());
            nodes.push((STATUS, status));
        }

        TreeUpdate {
            nodes,
            tree: Some(Tree::new(WINDOW)),
            tree_id: TreeId::ROOT,
            focus: self.workspace.as_ref().map_or(CONTENT, |workspace| {
                match self.workspace_focus {
                    WorkspaceFocus::Terminal => CONTENT,
                    WorkspaceFocus::Tabs => workspace
                        .tabs
                        .iter()
                        .position(|tab| tab.selected)
                        .map_or(CONTENT, tab_id),
                    WorkspaceFocus::Panes => workspace
                        .panes
                        .iter()
                        .position(|pane| pane.selected)
                        .map_or(CONTENT, pane_id),
                }
            }),
        }
    }
}

/// Direct activation callback that always returns the latest derived scene input.
pub struct Activation {
    snapshot: Arc<Mutex<Snapshot>>,
}

impl ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        Some(lock(&self.snapshot).tree())
    }
}

/// Latest accessibility input derived from the accepted scene, shared only for activation.
pub struct Accessibility {
    snapshot: Arc<Mutex<Snapshot>>,
}

impl Accessibility {
    #[must_use]
    pub fn new(size: PhysicalSize<u32>) -> Self {
        Self {
            snapshot: Arc::new(Mutex::new(Snapshot {
                title: "Venus".into(),
                content: AccessibleText::default(),
                status: "Connecting to Orbit".into(),
                size,
                terminal: false,
                workspace: None,
                workspace_focus: WorkspaceFocus::Terminal,
            })),
        }
    }

    #[must_use]
    pub fn activation(&self) -> Activation {
        Activation {
            snapshot: Arc::clone(&self.snapshot),
        }
    }

    pub fn update(
        &self,
        adapter: &mut Adapter,
        scene: Option<&Scene>,
        workspace: Option<&WorkspaceScene>,
        workspace_focus: WorkspaceFocus,
        status: &str,
        size: PhysicalSize<u32>,
    ) {
        {
            let mut snapshot = lock(&self.snapshot);
            snapshot.size = size;
            snapshot.status = status.to_owned();
            snapshot.workspace = workspace.cloned();
            snapshot.workspace_focus = workspace_focus;
            if let Some(scene) = scene {
                snapshot.title = if scene.title.is_empty() {
                    "Venus".into()
                } else {
                    scene.title.clone()
                };
                snapshot.content = scene.accessible_content();
                snapshot.terminal = true;
            } else {
                snapshot.title = "Venus".into();
                snapshot.content = AccessibleText::default();
                snapshot.terminal = false;
            }
        }
        adapter.update_if_active(|| lock(&self.snapshot).tree());
    }

    #[must_use]
    pub fn workspace_target(&self, target: NodeId) -> Option<AccessibilityTarget> {
        let snapshot = lock(&self.snapshot);
        let workspace = snapshot.workspace.as_ref()?;
        if target == CONTENT {
            return Some(AccessibilityTarget::Terminal);
        }
        if let Some((_, tab)) = workspace
            .tabs
            .iter()
            .enumerate()
            .find(|(index, _)| tab_id(*index) == target)
        {
            return Some(AccessibilityTarget::Tab(tab.id.clone()));
        }
        workspace
            .panes
            .iter()
            .enumerate()
            .find(|(index, _)| pane_id(*index) == target)
            .map(|(_, pane)| AccessibilityTarget::Pane(pane.id.clone()))
    }
}

/// Accessible workspace activation routed back through Eon semantics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccessibilityTarget {
    Terminal,
    Tab(String),
    Pane(String),
}

fn bounds(size: PhysicalSize<u32>) -> Rect {
    Rect {
        x0: 0.0,
        y0: 0.0,
        x1: f64::from(size.width),
        y1: f64::from(size.height),
    }
}

fn rect(rect: crate::SceneRect) -> Rect {
    Rect {
        x0: f64::from(rect.left),
        y0: f64::from(rect.top),
        x1: f64::from(rect.right()),
        y1: f64::from(rect.bottom()),
    }
}

fn tab_id(index: usize) -> NodeId {
    NodeId(100 + index as u64)
}

fn pane_id(index: usize) -> NodeId {
    NodeId(1_000 + index as u64)
}

fn text_run_id(index: usize) -> NodeId {
    NodeId(10_000 + index as u64)
}

fn text_position(position: AccessiblePosition) -> TextPosition {
    TextPosition {
        node: text_run_id(position.row),
        character_index: position.character_index,
    }
}

fn text_selection(selection: AccessibleSelection) -> TextSelection {
    TextSelection {
        anchor: text_position(selection.anchor),
        focus: text_position(selection.focus),
    }
}

fn text_run(row: &AccessibleRow, bounds: Rect, index: usize, count: usize) -> Node {
    let mut node = Node::new(Role::TextRun);
    node.set_value(row.value.as_str());
    node.set_character_lengths(row.character_lengths.clone());
    node.set_text_direction(TextDirection::LeftToRight);
    let height = (bounds.y1 - bounds.y0) / count.max(1) as f64;
    node.set_bounds(Rect {
        x0: bounds.x0,
        y0: bounds.y0 + height * index as f64,
        x1: bounds.x1,
        y1: bounds.y0 + height * (index + 1) as f64,
    });
    node
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CellMetrics;
    use eon_workspace_protocol::{Pane, Snapshot as WorkspaceSnapshot, Tab};

    #[test]
    fn scene_notice_preserves_terminal_content_and_adds_an_alert() {
        let update = Snapshot {
            title: "shell".into(),
            content: AccessibleText {
                rows: vec![AccessibleRow {
                    value: "terminal content".into(),
                    character_lengths: "terminal content"
                        .chars()
                        .map(|character| character.len_utf8() as u8)
                        .collect(),
                }],
                selection: None,
            },
            status: "renderer failure".into(),
            size: PhysicalSize::new(800, 600),
            terminal: true,
            workspace: None,
            workspace_focus: WorkspaceFocus::Terminal,
        }
        .tree();
        let node = |id| {
            &update
                .nodes
                .iter()
                .find(|(node_id, _)| *node_id == id)
                .unwrap()
                .1
        };

        assert_eq!(node(CONTENT).role(), Role::Terminal);
        assert_eq!(node(text_run_id(0)).value(), Some("terminal content"));
        assert_eq!(node(STATUS).role(), Role::Alert);
        assert_eq!(node(STATUS).value(), Some("renderer failure"));
        assert_eq!(node(WINDOW).children(), &[CONTENT, STATUS]);
    }

    #[test]
    fn authoritative_selection_uses_accesskit_text_positions() {
        let update = Snapshot {
            title: "shell".into(),
            content: AccessibleText {
                rows: vec![
                    AccessibleRow {
                        value: "one\n".into(),
                        character_lengths: vec![1, 1, 1, 1],
                    },
                    AccessibleRow {
                        value: "界".into(),
                        character_lengths: vec![3],
                    },
                ],
                selection: Some(AccessibleSelection {
                    anchor: AccessiblePosition {
                        row: 0,
                        character_index: 1,
                    },
                    focus: AccessiblePosition {
                        row: 1,
                        character_index: 1,
                    },
                }),
            },
            status: String::new(),
            size: PhysicalSize::new(800, 600),
            terminal: true,
            workspace: None,
            workspace_focus: WorkspaceFocus::Terminal,
        }
        .tree();
        let node = |id| {
            &update
                .nodes
                .iter()
                .find(|(node_id, _)| *node_id == id)
                .unwrap()
                .1
        };

        assert_eq!(node(CONTENT).children(), &[text_run_id(0), text_run_id(1)]);
        assert_eq!(node(text_run_id(0)).role(), Role::TextRun);
        assert_eq!(node(text_run_id(0)).value(), Some("one\n"));
        assert_eq!(
            node(CONTENT).text_selection(),
            Some(&TextSelection {
                anchor: TextPosition {
                    node: text_run_id(0),
                    character_index: 1,
                },
                focus: TextPosition {
                    node: text_run_id(1),
                    character_index: 1,
                },
            })
        );
    }

    #[test]
    fn workspace_accessibility_order_matches_tabs_then_one_expanded_pane() {
        let workspace = WorkspaceScene::from_snapshot(
            &WorkspaceSnapshot {
                active_tab: "tab-1".into(),
                tabs: vec![
                    Tab {
                        id: "tab-1".into(),
                        selected_pane: "pane-2".into(),
                        panes: vec![
                            Pane {
                                id: "pane-1".into(),
                                session: "session-1".into(),
                                endpoint: b"/run/eon/one.sock".to_vec(),
                                live: false,
                            },
                            Pane {
                                id: "pane-2".into(),
                                session: "session-2".into(),
                                endpoint: b"/run/eon/two.sock".to_vec(),
                                live: true,
                            },
                        ],
                    },
                    Tab {
                        id: "tab-2".into(),
                        selected_pane: "pane-3".into(),
                        panes: vec![Pane {
                            id: "pane-3".into(),
                            session: "session-3".into(),
                            endpoint: b"/run/eon/three.sock".to_vec(),
                            live: false,
                        }],
                    },
                ],
            },
            PhysicalSize::new(800, 600),
            CellMetrics::for_scale(1.0),
            0.0,
            0.0,
        );
        let terminal_layout_bounds = rect(workspace.terminal);
        let terminal_bounds = rect(workspace.visible_terminal().unwrap());
        let update = Snapshot {
            title: "shell".into(),
            content: AccessibleText {
                rows: vec![AccessibleRow {
                    value: "terminal".into(),
                    character_lengths: vec![1; 8],
                }],
                selection: None,
            },
            status: String::new(),
            size: PhysicalSize::new(800, 600),
            terminal: true,
            workspace: Some(workspace),
            workspace_focus: WorkspaceFocus::Panes,
        }
        .tree();
        let node = |id| {
            &update
                .nodes
                .iter()
                .find(|(node_id, _)| *node_id == id)
                .unwrap()
                .1
        };

        assert_eq!(node(WINDOW).children(), &[TAB_LIST, PANE_PANEL]);
        assert_eq!(node(TAB_LIST).children(), &[tab_id(0), tab_id(1)]);
        assert_eq!(
            node(PANE_PANEL).children(),
            &[pane_id(0), pane_id(1), CONTENT]
        );
        assert_eq!(node(tab_id(0)).role(), Role::Tab);
        assert_eq!(node(pane_id(0)).label(), Some("pane-1 offline"));
        assert_eq!(node(pane_id(1)).label(), Some("pane-2"));
        assert_eq!(node(pane_id(1)).role(), Role::Button);
        assert_eq!(node(pane_id(1)).is_expanded(), Some(true));
        assert_eq!(node(CONTENT).bounds(), Some(terminal_bounds));
        assert_eq!(node(text_run_id(0)).bounds(), Some(terminal_layout_bounds));
        assert_eq!(update.focus, pane_id(1));
    }
}
