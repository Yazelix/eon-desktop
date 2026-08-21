use crate::{
    CellMetrics, Scene, SceneRect, WorkspaceFocus, WorkspaceScene,
    scene::{AccessiblePosition, AccessibleRow, AccessibleSelection, AccessibleText},
};
use accesskit::{
    Action, ActivationHandler, Node, NodeId, Orientation, Rect, Role, TextDirection, TextPosition,
    TextSelection, Tree, TreeId, TreeUpdate,
};
use accesskit_winit::Adapter;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
};
use winit::dpi::PhysicalSize;

const WINDOW: NodeId = NodeId(0);
const CONTENT: NodeId = NodeId(1);
const STATUS: NodeId = NodeId(2);
const TAB_LIST: NodeId = NodeId(3);
const PANE_PANEL: NodeId = NodeId(4);
// Scene rows are u16, so this range cannot collide with fixed or text-run nodes.
const WORKSPACE_NODE_START: u64 = 1 << 32;

#[derive(Clone, Debug)]
struct Snapshot {
    title: String,
    content: AccessibleText,
    status: String,
    size: PhysicalSize<u32>,
    metrics: CellMetrics,
    columns: Option<u16>,
    workspace: Option<WorkspaceScene>,
    workspace_focus: WorkspaceFocus,
    tab_ids: HashMap<String, NodeId>,
    pane_ids: HashMap<String, NodeId>,
    next_workspace_id: u64,
}

impl Snapshot {
    fn new(size: PhysicalSize<u32>) -> Self {
        Self {
            title: "Venus".into(),
            content: AccessibleText::default(),
            status: "Connecting to Orbit".into(),
            size,
            metrics: CellMetrics::for_scale(1.0),
            columns: None,
            workspace: None,
            workspace_focus: WorkspaceFocus::Terminal,
            tab_ids: HashMap::new(),
            pane_ids: HashMap::new(),
            next_workspace_id: WORKSPACE_NODE_START,
        }
    }

    fn set_workspace(&mut self, workspace: Option<&WorkspaceScene>) {
        let Some(workspace) = workspace else {
            self.workspace = None;
            self.tab_ids.clear();
            self.pane_ids.clear();
            return;
        };

        self.tab_ids
            .retain(|id, _| workspace.tabs.iter().any(|tab| tab.id == *id));
        self.pane_ids
            .retain(|id, _| workspace.panes.iter().any(|pane| pane.id == *id));
        for tab in &workspace.tabs {
            allocate_workspace_id(&mut self.tab_ids, &mut self.next_workspace_id, &tab.id);
        }
        for pane in &workspace.panes {
            allocate_workspace_id(&mut self.pane_ids, &mut self.next_workspace_id, &pane.id);
        }
        self.workspace = Some(workspace.clone());
    }

    fn tree(&self) -> TreeUpdate {
        let terminal = self.columns.is_some();
        let mut root = Node::new(Role::Window);
        root.set_label(self.title.as_str());
        root.set_bounds(bounds(self.size));
        let status_alert = terminal && !self.status.is_empty();
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

        let mut content = Node::new(if terminal {
            Role::Terminal
        } else {
            Role::Alert
        });
        content.set_label(if terminal {
            self.title.as_str()
        } else {
            "Venus status"
        });
        if terminal {
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
        let terminal_layout = self.workspace.as_ref().map_or(
            SceneRect {
                left: 0.0,
                top: 0.0,
                width: self.size.width as f32,
                height: self.size.height as f32,
            },
            |workspace| workspace.terminal,
        );
        let visible_terminal = self
            .workspace
            .as_ref()
            .map_or(Some(terminal_layout), WorkspaceScene::visible_terminal);
        if terminal {
            content.set_clips_children();
        }
        if let Some(bounds) = visible_terminal {
            content.set_bounds(rect(bounds));
        }

        let mut nodes = vec![(WINDOW, root), (CONTENT, content)];
        if let Some(workspace) = &self.workspace {
            let mut tab_list = Node::new(Role::TabList);
            tab_list.set_label("Eon workspace tabs");
            tab_list.set_orientation(Orientation::Horizontal);
            tab_list.set_bounds(rect(workspace.tab_viewport));
            tab_list.set_clips_children();
            tab_list.set_children(
                workspace
                    .tabs
                    .iter()
                    .map(|tab| self.tab_ids[tab.id.as_str()])
                    .collect::<Vec<_>>(),
            );
            nodes.push((TAB_LIST, tab_list));
            for tab in &workspace.tabs {
                let mut node = Node::new(Role::Tab);
                node.set_label(tab.id.as_str());
                node.set_selected(tab.selected);
                if let Some(bounds) = tab.rect.intersection(workspace.tab_viewport) {
                    node.set_bounds(rect(bounds));
                }
                node.add_action(Action::Click);
                node.add_action(Action::Focus);
                nodes.push((self.tab_ids[tab.id.as_str()], node));
            }

            let mut panel = Node::new(Role::TabPanel);
            panel.set_label("Active tab panes");
            panel.set_orientation(Orientation::Vertical);
            panel.set_bounds(rect(workspace.pane_viewport));
            panel.set_clips_children();
            let mut children = Vec::with_capacity(workspace.panes.len() + 1);
            for pane in &workspace.panes {
                children.push(self.pane_ids[pane.id.as_str()]);
                if pane.selected {
                    children.push(CONTENT);
                }
            }
            panel.set_children(children);
            nodes.push((PANE_PANEL, panel));
            for pane in &workspace.panes {
                let mut node = Node::new(Role::Button);
                node.set_label(pane.label());
                node.set_selected(pane.selected);
                node.set_expanded(pane.selected);
                if let Some(bounds) = pane.rect.intersection(workspace.pane_viewport) {
                    node.set_bounds(rect(bounds));
                }
                node.add_action(Action::Click);
                node.add_action(Action::Focus);
                nodes.push((self.pane_ids[pane.id.as_str()], node));
            }
        }
        if let Some(columns) = self.columns {
            for (index, row) in self.content.rows.iter().enumerate() {
                let row_bounds = SceneRect {
                    left: terminal_layout.left + self.metrics.padding,
                    top: terminal_layout.top
                        + self.metrics.padding
                        + index as f32 * self.metrics.height,
                    width: f32::from(columns) * self.metrics.width,
                    height: self.metrics.height,
                };
                nodes.push((
                    text_run_id(index),
                    text_run(
                        row,
                        visible_terminal.and_then(|visible| row_bounds.intersection(visible)),
                    ),
                ));
            }
        }
        if status_alert {
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
                        .find(|tab| tab.selected)
                        .and_then(|tab| self.tab_ids.get(&tab.id).copied())
                        .unwrap_or(CONTENT),
                    WorkspaceFocus::Panes => workspace
                        .panes
                        .iter()
                        .find(|pane| pane.selected)
                        .and_then(|pane| self.pane_ids.get(&pane.id).copied())
                        .unwrap_or(CONTENT),
                }
            }),
        }
    }

    fn workspace_target(&self, target: NodeId) -> Option<AccessibilityTarget> {
        self.workspace.as_ref()?;
        if target == CONTENT {
            return Some(AccessibilityTarget::Terminal);
        }
        if let Some((id, _)) = self.tab_ids.iter().find(|(_, node)| **node == target) {
            return Some(AccessibilityTarget::Tab(id.clone()));
        }
        self.pane_ids
            .iter()
            .find(|(_, node)| **node == target)
            .map(|(id, _)| AccessibilityTarget::Pane(id.clone()))
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
            snapshot: Arc::new(Mutex::new(Snapshot::new(size))),
        }
    }

    #[must_use]
    pub fn activation(&self) -> Activation {
        Activation {
            snapshot: Arc::clone(&self.snapshot),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &self,
        adapter: &mut Adapter,
        scene: Option<&Scene>,
        workspace: Option<&WorkspaceScene>,
        workspace_focus: WorkspaceFocus,
        status: &str,
        size: PhysicalSize<u32>,
        metrics: CellMetrics,
    ) {
        {
            let mut snapshot = lock(&self.snapshot);
            snapshot.size = size;
            snapshot.metrics = metrics;
            snapshot.status = status.to_owned();
            snapshot.set_workspace(workspace);
            snapshot.workspace_focus = workspace_focus;
            if let Some(scene) = scene {
                snapshot.title = if scene.title.is_empty() {
                    "Venus".into()
                } else {
                    scene.title.clone()
                };
                snapshot.content = scene.accessible_content();
                snapshot.columns = Some(scene.columns);
            } else {
                snapshot.title = "Venus".into();
                snapshot.content = AccessibleText::default();
                snapshot.columns = None;
            }
        }
        adapter.update_if_active(|| lock(&self.snapshot).tree());
    }

    #[must_use]
    pub fn workspace_target(&self, target: NodeId) -> Option<AccessibilityTarget> {
        lock(&self.snapshot).workspace_target(target)
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

fn rect(rect: SceneRect) -> Rect {
    Rect {
        x0: f64::from(rect.left),
        y0: f64::from(rect.top),
        x1: f64::from(rect.right()),
        y1: f64::from(rect.bottom()),
    }
}

fn allocate_workspace_id(ids: &mut HashMap<String, NodeId>, next_id: &mut u64, identity: &str) {
    if ids.contains_key(identity) {
        return;
    }
    let following_id = next_id
        .checked_add(1)
        .expect("AccessKit workspace node IDs exhausted");
    ids.insert(identity.to_owned(), NodeId(*next_id));
    *next_id = following_id;
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

fn text_run(row: &AccessibleRow, bounds: Option<SceneRect>) -> Node {
    let mut node = Node::new(Role::TextRun);
    node.set_value(row.value.as_str());
    node.set_character_lengths(row.character_lengths.clone());
    node.set_text_direction(TextDirection::LeftToRight);
    if let Some(bounds) = bounds {
        node.set_bounds(rect(bounds));
    }
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
    use eon_workspace_protocol::{Pane, Snapshot as WorkspaceSnapshot, Tab};

    fn workspace_tab(id: &str, panes: &[&str], selected_pane: &str) -> Tab {
        Tab {
            id: id.into(),
            selected_pane: selected_pane.into(),
            panes: panes
                .iter()
                .map(|id| Pane {
                    id: (*id).into(),
                    session: format!("session-{id}"),
                    endpoint: format!("/run/eon/{id}.sock").into_bytes(),
                    live: true,
                })
                .collect(),
        }
    }

    fn workspace_scene(active_tab: &str, tabs: Vec<Tab>) -> WorkspaceScene {
        WorkspaceScene::from_snapshot(
            &WorkspaceSnapshot {
                active_tab: active_tab.into(),
                tabs,
            },
            PhysicalSize::new(800, 600),
            CellMetrics::for_scale(1.0),
            0.0,
            0.0,
        )
    }

    fn set_workspace(
        accessibility: &Accessibility,
        workspace: WorkspaceScene,
        focus: WorkspaceFocus,
    ) {
        let mut snapshot = lock(&accessibility.snapshot);
        snapshot.set_workspace(Some(&workspace));
        snapshot.workspace_focus = focus;
    }

    fn tree_node_id(update: &TreeUpdate, label: &str) -> NodeId {
        update
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some(label))
            .map(|(id, _)| *id)
            .unwrap()
    }

    fn node(update: &TreeUpdate, id: NodeId) -> &Node {
        &update
            .nodes
            .iter()
            .find(|(node_id, _)| *node_id == id)
            .unwrap()
            .1
    }

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
            columns: Some(16),
            ..Snapshot::new(PhysicalSize::new(800, 600))
        }
        .tree();
        assert_eq!(node(&update, CONTENT).role(), Role::Terminal);
        assert_eq!(
            node(&update, text_run_id(0)).value(),
            Some("terminal content")
        );
        assert_eq!(node(&update, STATUS).role(), Role::Alert);
        assert_eq!(node(&update, STATUS).value(), Some("renderer failure"));
        assert_eq!(node(&update, WINDOW).children(), &[CONTENT, STATUS]);
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
            columns: Some(4),
            ..Snapshot::new(PhysicalSize::new(800, 600))
        }
        .tree();
        assert_eq!(
            node(&update, CONTENT).children(),
            &[text_run_id(0), text_run_id(1)]
        );
        assert_eq!(node(&update, text_run_id(0)).role(), Role::TextRun);
        assert_eq!(node(&update, text_run_id(0)).value(), Some("one\n"));
        assert_eq!(
            node(&update, CONTENT).text_selection(),
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
    fn text_runs_follow_the_rendered_cell_grid() {
        let row = AccessibleRow {
            value: String::new(),
            character_lengths: Vec::new(),
        };
        let update = Snapshot {
            content: AccessibleText {
                rows: vec![row.clone(); 32],
                selection: None,
            },
            status: String::new(),
            columns: Some(93),
            ..Snapshot::new(PhysicalSize::new(960, 600))
        }
        .tree();
        assert!(node(&update, CONTENT).clips_children());
        assert_eq!(
            node(&update, text_run_id(0)).bounds(),
            Some(Rect {
                x0: 12.0,
                y0: 12.0,
                x1: 942.0,
                y1: 30.0,
            })
        );
        assert_eq!(
            node(&update, text_run_id(31)).bounds(),
            Some(Rect {
                x0: 12.0,
                y0: 570.0,
                x1: 942.0,
                y1: 588.0,
            })
        );

        for scale in [1.0, 1.25, 1.5, 2.0] {
            let metrics = CellMetrics::for_scale(scale);
            let update = Snapshot {
                content: AccessibleText {
                    rows: vec![row.clone(); 2],
                    selection: None,
                },
                status: String::new(),
                metrics,
                columns: Some(3),
                ..Snapshot::new(PhysicalSize::new(400, 300))
            }
            .tree();
            let second_row = node(&update, text_run_id(1)).bounds();

            assert_eq!(
                second_row,
                Some(rect(SceneRect {
                    left: metrics.padding,
                    top: metrics.padding + metrics.height,
                    width: metrics.width * 3.0,
                    height: metrics.height,
                })),
                "scale {scale}"
            );
        }
    }

    #[test]
    fn workspace_accessibility_uses_visible_intersections() {
        let columns = 4;
        let workspace_snapshot = WorkspaceSnapshot {
            active_tab: "tab-1".into(),
            tabs: vec![
                workspace_tab(
                    "tab-1",
                    &["pane-1", "pane-2", "pane-3", "pane-4", "pane-5", "pane-6"],
                    "pane-1",
                ),
                workspace_tab("tab-2", &["pane-7"], "pane-7"),
            ],
        };
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let metrics = CellMetrics::for_scale(scale);
            let size = PhysicalSize::new(100, (140.0 * scale) as u32);
            let workspace =
                WorkspaceScene::from_snapshot(&workspace_snapshot, size, metrics, 20.0, 20.0);
            let mut snapshot = Snapshot {
                content: AccessibleText {
                    rows: vec![
                        AccessibleRow {
                            value: "one\n".into(),
                            character_lengths: vec![1; 4],
                        },
                        AccessibleRow {
                            value: "two".into(),
                            character_lengths: vec![1; 3],
                        },
                    ],
                    selection: None,
                },
                status: String::new(),
                metrics,
                columns: Some(columns),
                workspace_focus: WorkspaceFocus::Panes,
                ..Snapshot::new(size)
            };
            snapshot.set_workspace(Some(&workspace));
            let tab = snapshot.tab_ids["tab-1"];
            let pane = snapshot.pane_ids["pane-1"];
            let update = snapshot.tree();
            assert!(node(&update, TAB_LIST).clips_children());
            assert!(node(&update, PANE_PANEL).clips_children());
            assert!(node(&update, CONTENT).clips_children());
            assert_eq!(
                node(&update, tab).bounds(),
                workspace.tabs[0]
                    .rect
                    .intersection(workspace.tab_viewport)
                    .map(rect)
            );
            assert_eq!(
                node(&update, pane).bounds(),
                workspace.panes[0]
                    .rect
                    .intersection(workspace.pane_viewport)
                    .map(rect)
            );
            assert_eq!(
                node(&update, CONTENT).bounds(),
                workspace.visible_terminal().map(rect)
            );
            let second_row = SceneRect {
                left: workspace.terminal.left + metrics.padding,
                top: workspace.terminal.top + metrics.padding + metrics.height,
                width: f32::from(columns) * metrics.width,
                height: metrics.height,
            }
            .intersection(workspace.visible_terminal().unwrap())
            .unwrap();
            assert_eq!(
                node(&update, text_run_id(1)).bounds(),
                Some(rect(second_row))
            );

            let fully_clipped = WorkspaceScene::from_snapshot(
                &workspace_snapshot,
                size,
                metrics,
                workspace.tab_scroll_limit(),
                workspace.pane_scroll_limit(),
            );
            assert_eq!(fully_clipped.visible_terminal(), None);
            snapshot.set_workspace(Some(&fully_clipped));
            let update = snapshot.tree();
            assert_eq!(node(&update, tab).bounds(), None);
            assert_eq!(node(&update, pane).bounds(), None);
            assert_eq!(node(&update, CONTENT).bounds(), None);
            assert_eq!(node(&update, text_run_id(0)).bounds(), None);
            assert!(node(&update, tab).supports_action(Action::Click));
            assert!(node(&update, tab).supports_action(Action::Focus));
            assert!(node(&update, pane).supports_action(Action::Click));
            assert!(node(&update, pane).supports_action(Action::Focus));
            assert_eq!(node(&update, text_run_id(0)).value(), Some("one\n"));
            assert_eq!(node(&update, PANE_PANEL).children().first(), Some(&pane));
            assert_eq!(update.focus, pane);
        }
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
        let terminal_bounds = rect(workspace.visible_terminal().unwrap());
        let mut snapshot = Snapshot {
            title: "shell".into(),
            content: AccessibleText {
                rows: vec![AccessibleRow {
                    value: "terminal".into(),
                    character_lengths: vec![1; 8],
                }],
                selection: None,
            },
            status: String::new(),
            columns: Some(8),
            workspace_focus: WorkspaceFocus::Panes,
            ..Snapshot::new(PhysicalSize::new(800, 600))
        };
        snapshot.set_workspace(Some(&workspace));
        let tab_1 = snapshot.tab_ids["tab-1"];
        let tab_2 = snapshot.tab_ids["tab-2"];
        let pane_1 = snapshot.pane_ids["pane-1"];
        let pane_2 = snapshot.pane_ids["pane-2"];
        let update = snapshot.tree();
        assert_eq!(node(&update, WINDOW).children(), &[TAB_LIST, PANE_PANEL]);
        assert_eq!(node(&update, TAB_LIST).children(), &[tab_1, tab_2]);
        assert_eq!(
            node(&update, PANE_PANEL).children(),
            &[pane_1, pane_2, CONTENT]
        );
        assert_eq!(node(&update, tab_1).role(), Role::Tab);
        assert_eq!(node(&update, pane_1).label(), Some("pane-1 offline"));
        assert_eq!(node(&update, pane_2).label(), Some("pane-2"));
        assert_eq!(node(&update, pane_2).role(), Role::Button);
        assert_eq!(node(&update, pane_2).is_expanded(), Some(true));
        assert_eq!(node(&update, CONTENT).bounds(), Some(terminal_bounds));
        assert_eq!(update.focus, pane_2);
    }

    #[test]
    fn workspace_nodes_keep_identity_and_retire_removed_actions() {
        let accessibility = Accessibility::new(PhysicalSize::new(800, 600));
        let mut activation = accessibility.activation();
        set_workspace(
            &accessibility,
            workspace_scene(
                "tab-b",
                vec![
                    workspace_tab("tab-a", &["pane-x"], "pane-x"),
                    workspace_tab("tab-b", &["pane-a", "pane-b"], "pane-b"),
                ],
            ),
            WorkspaceFocus::Panes,
        );
        let first = activation.request_initial_tree().unwrap();
        let tab_a = tree_node_id(&first, "tab-a");
        let tab_b = tree_node_id(&first, "tab-b");
        let pane_a = tree_node_id(&first, "pane-a");
        let pane_b = tree_node_id(&first, "pane-b");
        assert_ne!(tab_a, pane_a);

        set_workspace(
            &accessibility,
            workspace_scene("tab-b", vec![workspace_tab("tab-b", &["pane-b"], "pane-b")]),
            WorkspaceFocus::Panes,
        );
        let second = activation.request_initial_tree().unwrap();

        assert_eq!(tree_node_id(&second, "tab-b"), tab_b);
        assert_eq!(tree_node_id(&second, "pane-b"), pane_b);
        assert_eq!(second.focus, pane_b);
        assert_eq!(
            accessibility.workspace_target(tab_b),
            Some(AccessibilityTarget::Tab("tab-b".into()))
        );
        assert_eq!(
            accessibility.workspace_target(pane_b),
            Some(AccessibilityTarget::Pane("pane-b".into()))
        );
        assert_eq!(accessibility.workspace_target(tab_a), None);
        assert_eq!(accessibility.workspace_target(pane_a), None);
        assert_eq!(accessibility.workspace_target(NodeId(u64::MAX)), None);

        set_workspace(
            &accessibility,
            workspace_scene(
                "tab-b",
                vec![
                    workspace_tab("tab-c", &["pane-z"], "pane-z"),
                    workspace_tab("tab-b", &["pane-c", "pane-b"], "pane-b"),
                ],
            ),
            WorkspaceFocus::Panes,
        );
        let third = activation.request_initial_tree().unwrap();

        assert_eq!(tree_node_id(&third, "tab-b"), tab_b);
        assert_eq!(tree_node_id(&third, "pane-b"), pane_b);
        assert_ne!(tree_node_id(&third, "tab-c"), tab_a);
        assert_ne!(tree_node_id(&third, "pane-c"), pane_a);
        assert_eq!(third.focus, pane_b);
    }
}
