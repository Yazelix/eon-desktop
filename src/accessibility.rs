use crate::Scene;
use accesskit::{ActivationHandler, Node, NodeId, Rect, Role, Tree, TreeId, TreeUpdate};
use accesskit_winit::Adapter;
use std::sync::{Arc, Mutex, MutexGuard};
use winit::dpi::PhysicalSize;

const WINDOW: NodeId = NodeId(0);
const CONTENT: NodeId = NodeId(1);
const STATUS: NodeId = NodeId(2);

#[derive(Clone, Debug)]
struct Snapshot {
    title: String,
    value: String,
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
        content.set_value(if self.terminal {
            self.value.as_str()
        } else {
            self.status.as_str()
        });
        content.set_bounds(bounds(self.size));

        let mut nodes = vec![(WINDOW, root), (CONTENT, content)];
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
                value: String::new(),
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
                snapshot.value = scene.accessible_text();
                snapshot.terminal = true;
            } else {
                snapshot.title = "Venus".into();
                snapshot.value.clear();
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
            value: "terminal content".into(),
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
        assert_eq!(node(CONTENT).value(), Some("terminal content"));
        assert_eq!(node(STATUS).role(), Role::Alert);
        assert_eq!(node(STATUS).value(), Some("renderer failure"));
        assert_eq!(node(WINDOW).children(), &[CONTENT, STATUS]);
    }
}
