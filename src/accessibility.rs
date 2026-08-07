use crate::{
    Scene,
    scene::{AccessiblePosition, AccessibleRow, AccessibleSelection, AccessibleText},
};
use accesskit::{
    ActivationHandler, Node, NodeId, Rect, Role, TextDirection, TextPosition, TextSelection, Tree,
    TreeId, TreeUpdate,
};
use accesskit_winit::Adapter;
use std::sync::{Arc, Mutex, MutexGuard};
use winit::dpi::PhysicalSize;

const WINDOW: NodeId = NodeId(0);
const CONTENT: NodeId = NodeId(1);
const STATUS: NodeId = NodeId(2);

#[derive(Clone, Debug)]
struct Snapshot {
    title: String,
    content: AccessibleText,
    status: String,
    size: PhysicalSize<u32>,
    terminal: bool,
}

impl Snapshot {
    fn tree(&self) -> TreeUpdate {
        let mut root = Node::new(Role::Window);
        root.set_label(self.title.as_str());
        root.set_bounds(bounds(self.size));
        root.set_children(if self.terminal && !self.status.is_empty() {
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
        content.set_bounds(bounds(self.size));

        let mut nodes = vec![(WINDOW, root), (CONTENT, content)];
        if self.terminal {
            for (index, row) in self.content.rows.iter().enumerate() {
                nodes.push((
                    text_run_id(index),
                    text_run(row, self.size, index, self.content.rows.len()),
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
            focus: CONTENT,
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
        status: &str,
        size: PhysicalSize<u32>,
    ) {
        {
            let mut snapshot = lock(&self.snapshot);
            snapshot.size = size;
            snapshot.status = status.to_owned();
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
}

fn bounds(size: PhysicalSize<u32>) -> Rect {
    Rect {
        x0: 0.0,
        y0: 0.0,
        x1: f64::from(size.width),
        y1: f64::from(size.height),
    }
}

fn text_run_id(index: usize) -> NodeId {
    NodeId(10 + index as u64)
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

fn text_run(row: &AccessibleRow, size: PhysicalSize<u32>, index: usize, count: usize) -> Node {
    let mut node = Node::new(Role::TextRun);
    node.set_value(row.value.as_str());
    node.set_character_lengths(row.character_lengths.clone());
    node.set_text_direction(TextDirection::LeftToRight);
    let height = f64::from(size.height) / count.max(1) as f64;
    node.set_bounds(Rect {
        x0: 0.0,
        y0: height * index as f64,
        x1: f64::from(size.width),
        y1: height * (index + 1) as f64,
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
}
