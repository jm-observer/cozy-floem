use floem::peniko::Color;
use floem::prelude::{RwSignal, SignalGet, VirtualVector};
use floem::reactive::Scope;
use log::debug;
use crate::views::tree_with_panel::data::lines::TextSrc;

#[derive(Clone)]
pub enum TreeNode {
    Root {
        cx: Scope,
        children: Vec<TreeNode>,
        content: String,
        level: RwSignal<Level>,
        open: RwSignal<bool>
    },
    Node {
        id: TextSrc,
        content: String,
        level: RwSignal<Level>,
        open: RwSignal<bool>,
    },
}

#[derive(Clone, Debug)]
pub struct TreeNodeData {
    pub id: Option<TextSrc>,
    pub hash: Option<u64>,
    pub content: String,
    pub open: RwSignal<bool>,
    pub level: RwSignal<Level>,
}

#[derive(Clone, Debug)]
pub enum Level {
    None,
    Warn,
    Error,
}

impl TreeNodeData {

    pub fn track_level_svg(&self) -> &'static str {
        match self.level.get() {
            Level::None => {
                // empty.svg
                r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="16" height="16"></svg>"#
            }
            Level::Warn | Level::Error => {
                // warning.svg
                (r#"<svg width="16" height="16" viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="currentColor"><path fill-rule="evenodd" clip-rule="evenodd" d="M7.56 1h.88l6.54 12.26-.44.74H1.44L1 13.26 7.56 1zM8 2.28L2.28 13H13.7L8 2.28zM8.625 12v-1h-1.25v1h1.25zm-1.25-2V6h1.25v4h-1.25z"/></svg>"#)
            }
        }
    }
    pub fn track_level_svg_color(&self) -> Option<Color> {
        match self.level.get() {
            Level::None => {
                None
            }
            Level::Warn => {
                Some(Color::rgb8(255, 204, 102))
            }
            Level::Error => {
                Some(Color::rgb8(255, 153, 153))
            }
        }
    }
}

impl TreeNode {
    pub fn add_child(&mut self, child_id: TextSrc,
                     content: String) {
        debug!("add_child {:?}", child_id);
        if let TreeNode::Root { children, cx, .. } = self {
            if children.iter().find(|x| if let TreeNode::Node { id, .. } = x {
                *id == child_id
            } else {
                false
            }).is_none() {
                children.push(TreeNode::Node { id: child_id, content, level: cx.create_rw_signal(Level::None), open: cx.create_rw_signal(false) })
            }
        }
    }
    fn to_data(&self) -> TreeNodeData {
        match self {
            TreeNode::Root { content, open, level,.. } => {
                TreeNodeData {
                    id: None,
                    hash: None,
                    content: content.clone(),
                    open: *open,
                    level: *level,
                }
            }
            TreeNode::Node { id, content, open, level ,.. } => {
                TreeNodeData {
                    id: Some(id.clone()),
                    // todo
                    hash: None,
                    content: content.clone(),
                    open: *open,
                    level: *level,
                }
            }
        }
    }
    fn total(&self) -> usize {
        match self {
            TreeNode::Root { children, open, .. } => {
                if open.get() {
                    children.iter().fold(1, |mut total, x| {
                        total += x.total();
                        total
                    })
                } else {
                    1
                }
            }
            TreeNode::Node { .. } => { 1 }
        }
    }

    fn get_children(
        &self,
        min: usize,
        max: usize,
    ) -> Vec<(usize, usize, TreeNodeData)> {
        match self {
            TreeNode::Root { children, open, .. } => {
                let mut children_data = Vec::new();
                if min == 0 {
                    children_data.push((0, 0, self.to_data()));
                }
                if open.get() {
                    for (mut index, child) in children.iter().enumerate() {
                        index += 1;
                        if min <= index && index <= max {
                            children_data.push((index, 1, child.to_data()));
                        }
                    }
                }
                children_data
            }
            TreeNode::Node { .. } => { vec![] }
        }
    }
}


impl VirtualVector<(usize, usize, TreeNodeData)> for TreeNode {
    fn total_len(&self) -> usize {
        self.total()
    }

    fn slice(
        &mut self,
        range: std::ops::Range<usize>,
    ) -> impl Iterator<Item=(usize, usize, TreeNodeData)> {
        let min = range.start;
        let max = range.end;
        let children = self.get_children(min, max);
        children.into_iter()
    }
}
