use crate::{
    CellMetrics, MAX_LINK_BYTES, Scene, SceneRect, ShortcutViewerScene, WorkspaceFocus,
    WorkspaceHeaderControl, WorkspaceScene,
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
const SHORTCUT_DIALOG: NodeId = NodeId(5);
const NEW_TAB: NodeId = NodeId(6);
const SHORTCUTS: NodeId = NodeId(7);
const CLOSE_TAB: NodeId = NodeId(8);
const QUOTA: NodeId = NodeId(9);
// Scene rows are u16, so this range cannot collide with fixed or text-run nodes.
const WORKSPACE_NODE_START: u64 = 1 << 32;
const LINK_NODE_START: u64 = 1 << 48;
const SHORTCUT_NODE_START: u64 = 1 << 56;
const SHORTCUT_GROUP_STRIDE: u64 = 1 << 8;

#[derive(Clone, Debug)]
struct AccessibleLink {
    open: NodeId,
    copy: NodeId,
    row: u16,
    column: u16,
    label: String,
    bounds: SceneRect,
}

#[derive(Clone, Debug)]
struct Snapshot {
    title: String,
    content: AccessibleText,
    status: String,
    scrollback_label: Option<String>,
    size: PhysicalSize<u32>,
    metrics: CellMetrics,
    columns: Option<u16>,
    workspace: Option<WorkspaceScene>,
    shortcut_viewer: Option<ShortcutViewerScene>,
    workspace_focus: WorkspaceFocus,
    tab_ids: HashMap<String, NodeId>,
    pane_ids: HashMap<String, NodeId>,
    next_workspace_id: u64,
    links: Vec<AccessibleLink>,
    link_identity: Option<(u64, u64)>,
    next_link_id: u64,
}

impl Snapshot {
    fn new(size: PhysicalSize<u32>) -> Self {
        Self {
            title: "Venus".into(),
            content: AccessibleText::default(),
            status: "Connecting to Orbit".into(),
            scrollback_label: None,
            size,
            metrics: CellMetrics::for_scale(1.0),
            columns: None,
            workspace: None,
            shortcut_viewer: None,
            workspace_focus: WorkspaceFocus::Terminal,
            tab_ids: HashMap::new(),
            pane_ids: HashMap::new(),
            next_workspace_id: WORKSPACE_NODE_START,
            links: Vec::new(),
            link_identity: None,
            next_link_id: LINK_NODE_START,
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

    fn set_links(&mut self, scene: Option<&Scene>, generation: Option<u64>) {
        let identity = link_identity(scene, generation);
        if self.link_identity == identity {
            return;
        }
        self.links.clear();
        self.link_identity = identity;
        let Some((scene, _)) = scene.zip(generation) else {
            return;
        };
        let terminal = self.workspace.as_ref().map_or(
            SceneRect {
                width: self.size.width as f32,
                height: self.size.height as f32,
                ..SceneRect::default()
            },
            |workspace| workspace.terminal,
        );
        let visible = self
            .workspace
            .as_ref()
            .map_or(Some(terminal), WorkspaceScene::visible_terminal);
        self.links = visible.map_or_else(Vec::new, |visible| {
            scene
                .hyperlinks()
                .filter_map(|link| {
                    let bounds = link.rect(terminal, self.metrics).intersection(visible)?;
                    let open = NodeId(self.next_link_id);
                    let copy = NodeId(
                        self.next_link_id
                            .checked_add(1)
                            .expect("AccessKit link node IDs exhausted"),
                    );
                    self.next_link_id = self
                        .next_link_id
                        .checked_add(2)
                        .expect("AccessKit link node IDs exhausted");
                    Some(AccessibleLink {
                        open,
                        copy,
                        row: link.row,
                        column: link.column,
                        label: accessible_link_label(link.uri),
                        bounds,
                    })
                })
                .collect()
        });
    }

    fn tree(&self) -> TreeUpdate {
        let terminal = self.columns.is_some();
        let mut root = Node::new(Role::Window);
        root.set_label(self.title.as_str());
        root.set_bounds(bounds(self.size));
        if let Some(viewer) = &self.shortcut_viewer {
            root.set_children(vec![SHORTCUT_DIALOG]);
            let mut dialog = Node::new(Role::Dialog);
            dialog.set_label("Eon shortcuts");
            dialog.set_description(
                "Native Eon surface shortcuts. Use arrows or Page Up and Page Down to scroll; press Escape or Alt+Slash to close.",
            );
            dialog.set_bounds(rect(viewer.bounds));
            dialog.set_modal();
            dialog.set_clips_children();
            let mut nodes = vec![(WINDOW, root)];
            let mut group_ids = Vec::with_capacity(viewer.groups.len());
            for (group_index, group) in viewer.groups.iter().enumerate() {
                let group_id =
                    NodeId(SHORTCUT_NODE_START + group_index as u64 * SHORTCUT_GROUP_STRIDE);
                group_ids.push(group_id);
                let mut group_node = Node::new(Role::Group);
                group_node.set_label(group.title.as_str());
                if let Some(bounds) = group.heading.intersection(viewer.content) {
                    group_node.set_bounds(rect(bounds));
                }
                let row_ids = (0..group.rows.len())
                    .map(|index| NodeId(group_id.0 + index as u64 + 1))
                    .collect::<Vec<_>>();
                group_node.set_children(row_ids.clone());
                nodes.push((group_id, group_node));
                for (row, id) in group.rows.iter().zip(row_ids) {
                    let mut node = Node::new(Role::Label);
                    node.set_value(format!("{} — {}", row.shortcut, row.action));
                    if let Some(bounds) = row.rect.intersection(viewer.content) {
                        node.set_bounds(rect(bounds));
                    }
                    nodes.push((id, node));
                }
            }
            dialog.set_children(group_ids);
            nodes.push((SHORTCUT_DIALOG, dialog));
            return TreeUpdate {
                nodes,
                tree: Some(Tree::new(WINDOW)),
                tree_id: TreeId::ROOT,
                focus: SHORTCUT_DIALOG,
            };
        }
        let status_alert = terminal && !self.status.is_empty();
        root.set_children(if self.workspace.is_some() {
            let mut children = vec![TAB_LIST];
            if self
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.quota.is_some())
            {
                children.push(QUOTA);
            }
            children.extend([NEW_TAB, SHORTCUTS, CLOSE_TAB, PANE_PANEL]);
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
        } else if self.status.is_empty() {
            Role::GenericContainer
        } else {
            Role::Alert
        });
        content.set_label(if terminal {
            self.title.as_str()
        } else if self.status.is_empty() {
            "Tab content"
        } else {
            "Venus status"
        });
        if terminal {
            content.set_read_only();
            if let Some(label) = &self.scrollback_label {
                content.set_description(format!(
                    "Scrollback: {label} above live output (committed display rows)"
                ));
            }
            content.set_children(
                (0..self.content.rows.len())
                    .map(text_run_id)
                    .chain(self.links.iter().flat_map(|link| [link.open, link.copy]))
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
                node.set_label(tab.accessible_label());
                node.set_selected(tab.selected);
                if let Some(bounds) = tab.rect.intersection(workspace.tab_viewport) {
                    node.set_bounds(rect(bounds));
                }
                node.add_action(Action::Click);
                node.add_action(Action::Focus);
                nodes.push((self.tab_ids[tab.id.as_str()], node));
            }
            if let Some(quota) = &workspace.quota {
                let mut node = Node::new(Role::Label);
                node.set_value(quota.label.as_str());
                node.set_description(quota.description.as_str());
                node.set_bounds(rect(quota.rect));
                nodes.push((QUOTA, node));
            }
            for control in workspace.controls {
                let mut node = Node::new(Role::Button);
                node.set_label(control.kind.label());
                if control.kind == WorkspaceHeaderControl::CloseTab
                    && let Some(tab) = workspace.tabs.iter().find(|tab| tab.selected)
                {
                    node.set_description(format!("Active tab {}", tab.id));
                }
                node.set_bounds(rect(control.rect));
                node.add_action(Action::Click);
                node.add_action(Action::Focus);
                nodes.push((control_node_id(control.kind), node));
            }

            let mut panel = Node::new(Role::TabPanel);
            panel.set_label(
                workspace
                    .popup_label()
                    .unwrap_or(if workspace.panes.is_empty() {
                        "Empty tab"
                    } else {
                        "Active tab panes"
                    }),
            );
            panel.set_orientation(Orientation::Vertical);
            panel.set_bounds(rect(workspace.pane_viewport));
            panel.set_clips_children();
            let mut children = Vec::with_capacity(workspace.panes.len() + 1);
            if workspace.panes.is_empty() && (visible_terminal.is_some() || !self.status.is_empty())
            {
                children.push(CONTENT);
            }
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
        for link in &self.links {
            let mut open = Node::new(Role::Link);
            open.set_label(link.label.as_str());
            open.set_bounds(rect(link.bounds));
            open.add_action(Action::Click);
            nodes.push((link.open, open));

            // The locked AT-SPI adapter exposes Click but not CustomAction.
            let mut copy = Node::new(Role::Button);
            copy.set_label(format!("Copy {}", link.label));
            copy.add_action(Action::Click);
            nodes.push((link.copy, copy));
        }
        if status_alert {
            let mut status = Node::new(Role::Alert);
            status.set_label(self.status.as_str());
            nodes.push((STATUS, status));
        }

        TreeUpdate {
            nodes,
            tree: Some(Tree::new(WINDOW)),
            tree_id: TreeId::ROOT,
            focus: self.workspace.as_ref().map_or(CONTENT, |workspace| {
                let selected_tab = workspace
                    .tabs
                    .iter()
                    .find(|tab| tab.selected)
                    .and_then(|tab| self.tab_ids.get(&tab.id).copied())
                    .unwrap_or(CONTENT);
                let content = visible_terminal.map_or(selected_tab, |_| CONTENT);
                match self.workspace_focus {
                    WorkspaceFocus::Terminal => content,
                    WorkspaceFocus::Panes => workspace
                        .panes
                        .iter()
                        .find(|pane| pane.selected)
                        .and_then(|pane| self.pane_ids.get(&pane.id).copied())
                        .unwrap_or(content),
                    WorkspaceFocus::Tabs => selected_tab,
                    WorkspaceFocus::Header(control) => control_node_id(control),
                }
            }),
        }
    }

    fn action_target(&self, target: NodeId, action: Action) -> Option<AccessibilityTarget> {
        if self.shortcut_viewer.is_some() {
            return None;
        }
        if let Some(link) = self
            .links
            .iter()
            .find(|link| link.open == target || link.copy == target)
        {
            if action != Action::Click {
                return None;
            }
            return Some(AccessibilityTarget::Link {
                row: link.row,
                column: link.column,
                copy: link.copy == target,
            });
        }
        if !matches!(action, Action::Click | Action::Focus) {
            return None;
        }
        let workspace = self.workspace.as_ref()?;
        if let Some(control) = header_control_for_node(target) {
            return Some(AccessibilityTarget::HeaderControl {
                control,
                activate: action == Action::Click,
            });
        }
        if target == CONTENT {
            return workspace
                .visible_terminal()
                .map(|_| AccessibilityTarget::Terminal);
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
        shortcut_viewer: Option<&ShortcutViewerScene>,
        workspace_focus: WorkspaceFocus,
        status: &str,
        scrollback_label: Option<&str>,
        size: PhysicalSize<u32>,
        metrics: CellMetrics,
        link_generation: Option<u64>,
    ) {
        {
            let mut snapshot = lock(&self.snapshot);
            snapshot.size = size;
            snapshot.metrics = metrics;
            snapshot.status = status.to_owned();
            snapshot.scrollback_label = scrollback_label.map(str::to_owned);
            snapshot.set_workspace(workspace);
            snapshot.shortcut_viewer = shortcut_viewer.cloned();
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
            snapshot.set_links(scene, link_generation);
        }
        adapter.update_if_active(|| lock(&self.snapshot).tree());
    }

    pub fn present_links(
        &self,
        adapter: &mut Adapter,
        scene: Option<&Scene>,
        generation: Option<u64>,
    ) {
        if lock(&self.snapshot).link_identity == link_identity(scene, generation) {
            return;
        }
        adapter.update_if_active(|| {
            let mut snapshot = lock(&self.snapshot);
            snapshot.set_links(scene, generation);
            snapshot.tree()
        });
    }

    #[must_use]
    pub fn action_target(&self, target: NodeId, action: Action) -> Option<AccessibilityTarget> {
        lock(&self.snapshot).action_target(target, action)
    }
}

fn link_identity(scene: Option<&Scene>, generation: Option<u64>) -> Option<(u64, u64)> {
    scene
        .zip(generation)
        .map(|(scene, generation)| (generation, scene.revision))
}

fn accessible_link_label(uri: &str) -> String {
    if uri.len() > MAX_LINK_BYTES {
        format!("Link target exceeds {MAX_LINK_BYTES} bytes")
    } else {
        uri.escape_debug().to_string()
    }
}

/// Accessible action routed back through Venus and Eon semantics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccessibilityTarget {
    Terminal,
    Tab(String),
    Pane(String),
    HeaderControl {
        control: WorkspaceHeaderControl,
        activate: bool,
    },
    Link {
        row: u16,
        column: u16,
        copy: bool,
    },
}

fn control_node_id(control: WorkspaceHeaderControl) -> NodeId {
    match control {
        WorkspaceHeaderControl::NewTab => NEW_TAB,
        WorkspaceHeaderControl::Shortcuts => SHORTCUTS,
        WorkspaceHeaderControl::CloseTab => CLOSE_TAB,
    }
}

fn header_control_for_node(node: NodeId) -> Option<WorkspaceHeaderControl> {
    match node {
        NEW_TAB => Some(WorkspaceHeaderControl::NewTab),
        SHORTCUTS => Some(WorkspaceHeaderControl::Shortcuts),
        CLOSE_TAB => Some(WorkspaceHeaderControl::CloseTab),
        _ => None,
    }
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
    use crate::{Color, DrawCell, DrawRow, DrawStyle, PaneMetadata, ShortcutGroup, ShortcutRow};
    use eon_workspace_protocol::v6::{
        CodexQuota, CodexQuotaState, CodexQuotaWindow, Pane, Popup, PopupEntry, PopupGeometry,
        Shortcut, Snapshot as WorkspaceSnapshot, Tab,
    };
    use orbit_protocol::{CellWidth, Screen, Underline};

    fn workspace_tab(id: &str, panes: &[&str], selected_pane: &str) -> Tab {
        Tab {
            pending: false,
            selected_popup: None,
            popups: Vec::new(),
            id: id.into(),
            directory: format!("/tmp/{id}").into_bytes(),
            selected_pane: Some(selected_pane.into()),
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
                geometry: eon_workspace_protocol::v6::PopupGeometry {
                    side_margin: 8.0,
                    vertical_margin: 4.0,
                },
                entries: Vec::new(),
                codex_quota: None,
            },
            PhysicalSize::new(800, 600),
            CellMetrics::for_scale(1.0),
            0.0,
            0.0,
            |_, text| (text.to_owned(), text.chars().count() as f32 * 10.0),
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
    fn quota_chip_preserves_truth_layout_and_static_accessibility() {
        let windows = || {
            vec![
                CodexQuotaWindow {
                    duration_minutes: 5 * 60,
                    remaining_percent: 74,
                    resets_at: None,
                },
                CodexQuotaWindow {
                    duration_minutes: 7 * 24 * 60,
                    remaining_percent: 61,
                    resets_at: Some(1_800_003_600),
                },
            ]
        };
        let quota = |state, windows| {
            Some(CodexQuota {
                state,
                observed_at: 1_800_000_000,
                windows,
            })
        };
        let cases = [
            (
                "fresh wide",
                800,
                quota(CodexQuotaState::Fresh, windows()),
                Some("Codex 5h 74% · 7d 61%"),
                "state: fresh",
            ),
            (
                "fresh compact",
                400,
                quota(CodexQuotaState::Fresh, windows()),
                Some("Codex 61%"),
                "reset time unavailable",
            ),
            (
                "fresh unusual duration",
                800,
                quota(
                    CodexQuotaState::Fresh,
                    vec![CodexQuotaWindow {
                        duration_minutes: 90,
                        remaining_percent: 50,
                        resets_at: Some(1_800_003_600),
                    }],
                ),
                Some("Codex 90m 50%"),
                "90m: 50% remaining",
            ),
            (
                "stale compact",
                400,
                quota(
                    CodexQuotaState::Stale,
                    windows()
                        .into_iter()
                        .map(|window| CodexQuotaWindow {
                            resets_at: window.resets_at.or(Some(1_800_007_200)),
                            ..window
                        })
                        .collect(),
                ),
                Some("Codex 61% old"),
                "state: stale",
            ),
            (
                "stale hidden",
                300,
                quota(
                    CodexQuotaState::Stale,
                    windows()
                        .into_iter()
                        .map(|window| CodexQuotaWindow {
                            resets_at: window.resets_at.or(Some(1_800_007_200)),
                            ..window
                        })
                        .collect(),
                ),
                None,
                "",
            ),
            (
                "blocked",
                400,
                quota(CodexQuotaState::Blocked, Vec::new()),
                Some("Codex blocked"),
                "permission: blocked",
            ),
            (
                "unknown",
                400,
                quota(CodexQuotaState::Unknown, Vec::new()),
                Some("Codex unknown"),
                "permission: unknown",
            ),
            ("absent", 800, None, None, ""),
        ];

        for (name, width, codex_quota, expected_label, detail) in cases {
            let workspace_snapshot = WorkspaceSnapshot {
                active_tab: "t1".into(),
                geometry: PopupGeometry {
                    side_margin: 8.0,
                    vertical_margin: 4.0,
                },
                entries: Vec::new(),
                tabs: vec![workspace_tab("t1", &["p1"], "p1")],
                codex_quota,
            };
            let size = PhysicalSize::new(width, 600);
            let metrics = CellMetrics::for_scale(1.0);
            let project = |snapshot: &WorkspaceSnapshot| {
                WorkspaceScene::from_snapshot(snapshot, size, metrics, 0.0, 0.0, |_, text| {
                    (text.to_owned(), text.chars().count() as f32 * 10.0)
                })
            };
            let workspace = project(&workspace_snapshot);
            let mut without_quota = workspace_snapshot.clone();
            without_quota.codex_quota = None;
            let baseline = project(&without_quota);

            assert_eq!(
                workspace.quota.as_ref().map(|quota| quota.label.as_str()),
                expected_label,
                "{name}"
            );
            if let Some(quota) = &workspace.quota {
                assert!(quota.description.contains(detail), "{name}");
                assert!(
                    quota
                        .description
                        .contains("Last observed at Unix time 1800000000")
                );
                assert_eq!(
                    workspace.hit_test(
                        quota.rect.left + quota.rect.width / 2.0,
                        quota.rect.height / 2.0
                    ),
                    Some(crate::WorkspaceHit::Quota),
                    "{name}"
                );
                assert!(
                    quota.rect.right() <= workspace.controls[0].rect.left,
                    "{name}"
                );
            } else {
                assert_eq!(workspace.tab_viewport, baseline.tab_viewport, "{name}");
                assert_eq!(workspace.drag_region, baseline.drag_region, "{name}");
                assert_eq!(workspace.controls, baseline.controls, "{name}");
            }

            let mut accessible = Snapshot::new(size);
            accessible.set_workspace(Some(&workspace));
            let update = accessible.tree();
            let quota_node = update.nodes.iter().find(|(id, _)| *id == QUOTA);
            assert_eq!(quota_node.is_some(), expected_label.is_some(), "{name}");
            if let Some((_, node)) = quota_node {
                assert_eq!(node.role(), Role::Label, "{name}");
                assert_eq!(node.value(), expected_label, "{name}");
                assert!(
                    node.description()
                        .is_some_and(|value| value.contains(detail))
                );
                assert!(!node.supports_action(Action::Click), "{name}");
                assert!(!node.supports_action(Action::Focus), "{name}");
            }
        }
    }

    fn linked_scene(revision: u64, uri: &str) -> Scene {
        Scene {
            revision,
            columns: 1,
            rows: 1,
            screen: Screen::Primary,
            title: "shell".into(),
            working_directory: String::new(),
            background: Color::default(),
            foreground: Color::default(),
            cursor: None,
            content: vec![DrawRow {
                wrapped: false,
                wrap_continuation: false,
                kitty_virtual_placeholder: false,
                cells: vec![DrawCell {
                    width: CellWidth::Narrow,
                    text: "x".into(),
                    hyperlink: uri.into(),
                    style: DrawStyle {
                        foreground: Color::default(),
                        background: Color::default(),
                        underline_color: Color::default(),
                        bold: false,
                        italic: false,
                        faint: false,
                        blink: false,
                        invisible: false,
                        strikethrough: false,
                        overline: false,
                        selected: false,
                        background_is_default: true,
                        protected: false,
                        underline: Underline::None,
                    },
                }],
            }],
        }
    }

    #[test]
    fn shortcut_viewer_replaces_terminal_tree_with_a_focused_dialog() {
        let size = PhysicalSize::new(320, 180);
        let viewer = ShortcutViewerScene::new(
            vec![ShortcutGroup::new(
                "Host",
                vec![ShortcutRow::new("Alt+/", "Show or close shortcuts")],
            )],
            size,
            CellMetrics::for_scale(1.0),
            0.0,
        );
        let mut snapshot = Snapshot::new(size);
        snapshot.shortcut_viewer = Some(viewer);
        let update = snapshot.tree();

        assert_eq!(node(&update, WINDOW).children(), &[SHORTCUT_DIALOG]);
        assert_eq!(node(&update, SHORTCUT_DIALOG).role(), Role::Dialog);
        assert!(node(&update, SHORTCUT_DIALOG).is_modal());
        assert_eq!(update.focus, SHORTCUT_DIALOG);
        tree_node_id(&update, "Host");
        assert!(
            update
                .nodes
                .iter()
                .any(|(_, node)| { node.value() == Some("Alt+/ — Show or close shortcuts") })
        );
        assert_eq!(snapshot.action_target(CONTENT, Action::Focus), None);
    }

    #[test]
    fn visible_links_offer_only_open_and_copy_and_retire_with_the_scene() {
        let mut snapshot = Snapshot::new(PhysicalSize::new(800, 600));
        let first_scene = linked_scene(1, "https://example.com/one");
        snapshot.content = first_scene.accessible_content();
        snapshot.columns = Some(first_scene.columns);
        snapshot.status.clear();
        snapshot.set_links(Some(&first_scene), Some(7));

        let first = snapshot.tree();
        let first_link = tree_node_id(&first, "https://example.com/one");
        let first_copy = tree_node_id(&first, "Copy https://example.com/one");
        assert_eq!(node(&first, first_link).role(), Role::Link);
        assert!(node(&first, first_link).supports_action(Action::Click));
        assert!(!node(&first, first_link).supports_action(Action::CustomAction));
        assert!(!node(&first, first_link).supports_action(Action::Focus));
        assert_eq!(node(&first, first_copy).role(), Role::Button);
        assert!(node(&first, first_copy).supports_action(Action::Click));
        assert!(!node(&first, first_copy).supports_action(Action::CustomAction));
        assert!(!node(&first, first_copy).supports_action(Action::Focus));
        assert_eq!(
            snapshot.action_target(first_link, Action::Click),
            Some(AccessibilityTarget::Link {
                row: 0,
                column: 0,
                copy: false,
            })
        );
        assert_eq!(
            snapshot.action_target(first_copy, Action::Click),
            Some(AccessibilityTarget::Link {
                row: 0,
                column: 0,
                copy: true,
            })
        );
        assert_eq!(snapshot.action_target(first_copy, Action::Focus), None);

        snapshot.set_links(Some(&first_scene), None);
        assert_eq!(snapshot.action_target(first_link, Action::Click), None);
        assert_eq!(snapshot.action_target(first_copy, Action::Click), None);

        let second_scene = linked_scene(1, "https://example.com/two");
        snapshot.content = second_scene.accessible_content();
        snapshot.columns = Some(second_scene.columns);
        snapshot.set_links(Some(&second_scene), Some(8));
        let second = snapshot.tree();
        let second_link = tree_node_id(&second, "https://example.com/two");
        assert_ne!(first_link, second_link);
        assert_eq!(snapshot.action_target(first_link, Action::Click), None);

        let oversized = "x".repeat(MAX_LINK_BYTES + 1);
        let oversized_scene = linked_scene(2, &oversized);
        snapshot.set_links(Some(&oversized_scene), Some(9));
        let oversized = snapshot.tree();
        let failure = format!("Link target exceeds {MAX_LINK_BYTES} bytes");
        tree_node_id(&oversized, &failure);
        tree_node_id(&oversized, &format!("Copy {failure}"));
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
        assert_eq!(node(&update, STATUS).label(), Some("renderer failure"));
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

        let fonts = crate::FontSetup::new(&crate::FontSettings {
            family: Some("DejaVu Sans Mono".into()),
            size: 20.0,
            line_height: 1.5,
            ..Default::default()
        })
        .unwrap();
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let metrics = fonts.metrics(scale);
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
            active_tab: "t1".into(),
            tabs: vec![
                workspace_tab(
                    "t1",
                    &["pane-1", "pane-2", "pane-3", "pane-4", "pane-5", "pane-6"],
                    "pane-1",
                ),
                workspace_tab("t2", &["pane-7"], "pane-7"),
                workspace_tab("t3", &["pane-8"], "pane-8"),
            ],
            geometry: eon_workspace_protocol::v6::PopupGeometry {
                side_margin: 8.0,
                vertical_margin: 4.0,
            },
            entries: Vec::new(),
            codex_quota: None,
        };
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let metrics = CellMetrics::for_scale(scale);
            let size = PhysicalSize::new(100, (140.0 * scale) as u32);
            let workspace = WorkspaceScene::from_snapshot(
                &workspace_snapshot,
                size,
                metrics,
                20.0,
                20.0,
                |_, text| (text.to_owned(), text.chars().count() as f32 * 10.0),
            );
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
            let tab = snapshot.tab_ids["t1"];
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
                |_, text| (text.to_owned(), text.chars().count() as f32 * 10.0),
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
    fn eon_bar_buttons_share_scene_bounds_and_separate_focus_from_activation() {
        let workspace = workspace_scene(
            "t2",
            vec![
                workspace_tab("t1", &["pane-a"], "pane-a"),
                workspace_tab("t2", &["pane-b"], "pane-b"),
            ],
        );
        let mut snapshot = Snapshot::new(PhysicalSize::new(800, 600));
        snapshot.set_workspace(Some(&workspace));

        for (id, control, label) in [
            (
                NEW_TAB,
                WorkspaceHeaderControl::NewTab,
                "New tab — Alt+Shift+T",
            ),
            (
                SHORTCUTS,
                WorkspaceHeaderControl::Shortcuts,
                "Keyboard shortcuts — Alt+/",
            ),
            (
                CLOSE_TAB,
                WorkspaceHeaderControl::CloseTab,
                "Close tab — Alt+Shift+W",
            ),
        ] {
            let update = snapshot.tree();
            let control_node = node(&update, id);
            assert_eq!(control_node.role(), Role::Button);
            assert_eq!(control_node.label(), Some(label));
            assert_eq!(
                control_node.bounds(),
                Some(rect(
                    workspace
                        .controls
                        .into_iter()
                        .find(|candidate| candidate.kind == control)
                        .unwrap()
                        .rect,
                ))
            );
            assert!(control_node.supports_action(Action::Focus));
            assert!(control_node.supports_action(Action::Click));
            assert_eq!(
                snapshot.action_target(id, Action::Focus),
                Some(AccessibilityTarget::HeaderControl {
                    control,
                    activate: false,
                })
            );
            assert_eq!(
                snapshot.action_target(id, Action::Click),
                Some(AccessibilityTarget::HeaderControl {
                    control,
                    activate: true,
                })
            );
            snapshot.workspace_focus = WorkspaceFocus::Header(control);
            assert_eq!(snapshot.tree().focus, id);
        }

        assert_eq!(
            node(&snapshot.tree(), CLOSE_TAB).description(),
            Some("Active tab t2")
        );
    }

    #[test]
    fn workspace_accessibility_order_matches_tabs_then_one_expanded_pane() {
        let metadata = PaneMetadata::Available {
            working_directory: "file:///tmp/eon".into(),
        };
        let workspace = WorkspaceScene::from_snapshot_with_metadata(
            &WorkspaceSnapshot {
                active_tab: "t1".into(),
                tabs: vec![
                    Tab {
                        pending: false,
                        selected_popup: None,
                        popups: Vec::new(),
                        id: "t1".into(),
                        directory: b"/tmp/eon".to_vec(),
                        selected_pane: Some("p2".into()),
                        panes: vec![
                            Pane {
                                id: "p1".into(),
                                session: "session-1".into(),
                                endpoint: b"/run/eon/one.sock".to_vec(),
                                live: false,
                            },
                            Pane {
                                id: "p2".into(),
                                session: "session-2".into(),
                                endpoint: b"/run/eon/two.sock".to_vec(),
                                live: true,
                            },
                        ],
                    },
                    Tab {
                        pending: false,
                        selected_popup: None,
                        popups: Vec::new(),
                        id: "t2".into(),
                        directory: b"/tmp/nova".to_vec(),
                        selected_pane: Some("p3".into()),
                        panes: vec![Pane {
                            id: "p3".into(),
                            session: "session-3".into(),
                            endpoint: b"/run/eon/three.sock".to_vec(),
                            live: false,
                        }],
                    },
                ],
                geometry: eon_workspace_protocol::v6::PopupGeometry {
                    side_margin: 8.0,
                    vertical_margin: 4.0,
                },
                entries: Vec::new(),
                codex_quota: None,
            },
            PhysicalSize::new(800, 600),
            CellMetrics::for_scale(1.0),
            0.0,
            0.0,
            |endpoint| (endpoint == b"/run/eon/two.sock").then_some(&metadata),
            |_, text| (text.to_owned(), text.chars().count() as f32 * 10.0),
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
        let tab_1 = snapshot.tab_ids["t1"];
        let tab_2 = snapshot.tab_ids["t2"];
        let pane_1 = snapshot.pane_ids["p1"];
        let pane_2 = snapshot.pane_ids["p2"];
        let update = snapshot.tree();
        assert_eq!(
            node(&update, WINDOW).children(),
            &[TAB_LIST, NEW_TAB, SHORTCUTS, CLOSE_TAB, PANE_PANEL]
        );
        assert_eq!(node(&update, TAB_LIST).children(), &[tab_1, tab_2]);
        assert_eq!(
            node(&update, PANE_PANEL).children(),
            &[pane_1, pane_2, CONTENT]
        );
        assert_eq!(node(&update, tab_1).role(), Role::Tab);
        assert_eq!(node(&update, tab_1).label(), Some("Tab 1 of 2  /tmp/eon"));
        assert_eq!(node(&update, tab_2).label(), Some("Tab 2 of 2  /tmp/nova"));
        assert_eq!(node(&update, pane_1).label(), Some("p1 offline"));
        assert_eq!(workspace.panes[1].label(), "p2  /tmp/eon");
        assert_eq!(
            node(&update, pane_2).label(),
            Some(workspace.panes[1].label())
        );
        assert_eq!(node(&update, pane_2).role(), Role::Button);
        assert_eq!(node(&update, pane_2).is_expanded(), Some(true));
        assert_eq!(node(&update, CONTENT).bounds(), Some(terminal_bounds));
        assert_eq!(update.focus, pane_2);
    }

    #[test]
    fn popup_and_empty_body_expose_only_visible_accessibility_targets() {
        let size = PhysicalSize::new(800, 600);
        let mut state = WorkspaceSnapshot {
            active_tab: "t1".into(),
            geometry: PopupGeometry {
                side_margin: 8.0,
                vertical_margin: 4.0,
            },
            entries: vec![PopupEntry {
                id: "project".into(),
                label: "Project".into(),
                shortcut: Shortcut {
                    modifiers: eon_workspace_protocol::v6::ALT,
                    key: "KeyZ".into(),
                },
            }],
            tabs: vec![Tab {
                id: "t1".into(),
                directory: b"/tmp/t1".to_vec(),
                pending: true,
                selected_pane: None,
                panes: Vec::new(),
                selected_popup: Some("u1".into()),
                popups: vec![Popup {
                    id: "u1".into(),
                    entry: "project".into(),
                    session: "s1".into(),
                    endpoint: b"/run/popup.sock".to_vec(),
                }],
            }],
            codex_quota: None,
        };
        let workspace = WorkspaceScene::from_snapshot(
            &state,
            size,
            CellMetrics::for_scale(1.0),
            0.0,
            0.0,
            |_, text| (text.into(), text.len() as f32 * 10.0),
        );
        let mut snapshot = Snapshot {
            content: AccessibleText {
                rows: vec![AccessibleRow {
                    value: "popup".into(),
                    character_lengths: vec![1; 5],
                }],
                selection: None,
            },
            columns: Some(6),
            workspace_focus: WorkspaceFocus::Panes,
            ..Snapshot::new(size)
        };
        snapshot.set_workspace(Some(&workspace));
        let tab = snapshot.tab_ids["t1"];
        let update = snapshot.tree();

        assert_eq!(node(&update, PANE_PANEL).label(), Some("Project"));
        assert_eq!(node(&update, PANE_PANEL).children(), &[CONTENT]);
        assert_eq!(update.focus, CONTENT);
        assert!(node(&update, tab).supports_action(Action::Click));
        assert!(node(&update, tab).supports_action(Action::Focus));
        assert_eq!(
            snapshot.action_target(tab, Action::Click),
            Some(AccessibilityTarget::Tab("t1".into()))
        );
        snapshot.workspace_focus = WorkspaceFocus::Tabs;
        assert_eq!(snapshot.tree().focus, tab);
        state.tabs[0].pending = false;
        state.tabs[0].selected_popup = None;
        snapshot.columns = None;
        snapshot.content = AccessibleText::default();
        snapshot.status.clear();
        snapshot.set_workspace(Some(&WorkspaceScene::from_snapshot(
            &state,
            size,
            CellMetrics::for_scale(1.0),
            0.0,
            0.0,
            |_, text| (text.into(), text.len() as f32 * 10.0),
        )));
        snapshot.workspace_focus = WorkspaceFocus::Terminal;
        let empty = snapshot.tree();
        assert_eq!(node(&empty, CONTENT).role(), Role::GenericContainer);
        assert!(node(&empty, CONTENT).children().is_empty());
        assert!(node(&empty, PANE_PANEL).children().is_empty());
        assert_eq!(node(&empty, PANE_PANEL).label(), Some("Empty tab"));
        assert_eq!(empty.focus, tab);
        assert!(node(&empty, tab).supports_action(Action::Click));
        snapshot.workspace_focus = WorkspaceFocus::Panes;
        assert_eq!(snapshot.action_target(CONTENT, Action::Click), None);
        snapshot.status = "popup action rejected".into();
        let failed = snapshot.tree();
        assert_eq!(node(&failed, PANE_PANEL).children(), &[CONTENT]);
        assert_eq!(node(&failed, CONTENT).role(), Role::Alert);
        assert_eq!(
            node(&failed, CONTENT).value(),
            Some("popup action rejected")
        );
        assert_eq!(failed.focus, tab);
        snapshot.status.clear();

        state.tabs.push(workspace_tab("t2", &["p2"], "p2"));
        state.active_tab = "t2".into();
        let inactive_popup = WorkspaceScene::from_snapshot(
            &state,
            size,
            CellMetrics::for_scale(1.0),
            0.0,
            0.0,
            |_, text| (text.into(), text.len() as f32 * 10.0),
        );
        snapshot.set_workspace(Some(&inactive_popup));
        let active_tab = snapshot.tab_ids["t2"];
        let pane = snapshot.pane_ids["p2"];
        let update = snapshot.tree();

        assert_eq!(node(&update, PANE_PANEL).label(), Some("Active tab panes"));
        assert_eq!(node(&update, PANE_PANEL).children(), &[pane, CONTENT]);
        assert_eq!(update.focus, pane);
        assert!(node(&update, active_tab).supports_action(Action::Focus));
        assert_eq!(
            snapshot.action_target(active_tab, Action::Click),
            Some(AccessibilityTarget::Tab("t2".into()))
        );
    }

    #[test]
    fn workspace_nodes_keep_identity_and_retire_removed_actions() {
        let accessibility = Accessibility::new(PhysicalSize::new(800, 600));
        let mut activation = accessibility.activation();
        set_workspace(
            &accessibility,
            workspace_scene(
                "t2",
                vec![
                    workspace_tab("t1", &["pane-x"], "pane-x"),
                    workspace_tab("t2", &["pane-a", "pane-b"], "pane-b"),
                ],
            ),
            WorkspaceFocus::Panes,
        );
        let first = activation.request_initial_tree().unwrap();
        let tab_a = tree_node_id(&first, "Tab 1 of 2  /tmp/t1");
        let tab_b = tree_node_id(&first, "Tab 2 of 2  /tmp/t2");
        let pane_a = tree_node_id(&first, "pane-a unavailable");
        let pane_b = tree_node_id(&first, "pane-b unavailable");
        assert_ne!(tab_a, pane_a);

        set_workspace(
            &accessibility,
            workspace_scene("t2", vec![workspace_tab("t2", &["pane-b"], "pane-b")]),
            WorkspaceFocus::Panes,
        );
        let second = activation.request_initial_tree().unwrap();

        assert_eq!(tree_node_id(&second, "Tab 1 of 1  /tmp/t2"), tab_b);
        assert_eq!(tree_node_id(&second, "pane-b unavailable"), pane_b);
        assert_eq!(second.focus, pane_b);
        assert_eq!(
            accessibility.action_target(tab_b, Action::Click),
            Some(AccessibilityTarget::Tab("t2".into()))
        );
        assert_eq!(
            accessibility.action_target(pane_b, Action::Click),
            Some(AccessibilityTarget::Pane("pane-b".into()))
        );
        assert_eq!(accessibility.action_target(tab_a, Action::Click), None);
        assert_eq!(accessibility.action_target(pane_a, Action::Click), None);
        assert_eq!(
            accessibility.action_target(NodeId(u64::MAX), Action::Click),
            None
        );

        set_workspace(
            &accessibility,
            workspace_scene(
                "t2",
                vec![
                    workspace_tab("t3", &["pane-z"], "pane-z"),
                    workspace_tab("t2", &["pane-c", "pane-b"], "pane-b"),
                ],
            ),
            WorkspaceFocus::Panes,
        );
        let third = activation.request_initial_tree().unwrap();

        assert_eq!(tree_node_id(&third, "Tab 2 of 2  /tmp/t2"), tab_b);
        assert_eq!(tree_node_id(&third, "pane-b unavailable"), pane_b);
        assert_ne!(tree_node_id(&third, "Tab 1 of 2  /tmp/t3"), tab_a);
        assert_ne!(tree_node_id(&third, "pane-c unavailable"), pane_a);
        assert_eq!(third.focus, pane_b);
    }

    #[test]
    fn scrollback_is_a_terminal_description_without_live_alerts() {
        let mut snapshot = Snapshot::new(PhysicalSize::new(800, 600));
        snapshot.columns = Some(80);
        snapshot.status.clear();
        for label in [Some("↑ 240 rows"), Some("↑ 1 row"), None] {
            snapshot.scrollback_label = label.map(str::to_owned);
            let update = snapshot.tree();
            assert_eq!(node(&update, CONTENT).role(), Role::Terminal);
            assert_eq!(
                node(&update, CONTENT).description(),
                label
                    .map(|label| format!(
                        "Scrollback: {label} above live output (committed display rows)"
                    ))
                    .as_deref()
            );
            assert!(
                update
                    .nodes
                    .iter()
                    .all(|(_, node)| node.role() != Role::Alert)
            );
        }
    }
}
