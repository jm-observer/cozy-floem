use floem::prelude::VirtualVector;
use crate::views::tree_with_panel::data::lines::TextSrc;

#[derive(Clone)]
pub enum TreeNode {
    Root  {
        children: Vec<TreeNode>,
        content: String,
    },
    Node {
        id: TextSrc,
        content: String,
    }
}

#[derive(Clone, Debug)]
pub struct TreeNodeData {
    pub id: Option<TextSrc>,
    pub hash: Option<u64>,
    pub content: String,
}

impl TreeNode {

    pub fn add_child(&mut self, child_id: TextSrc,
                     content: String) {
        match self {
            TreeNode::Root { children, .. } => {
                if children.iter().find(|x| if let TreeNode::Node { id, ..} = x {
                    *id == child_id
                } else {
                    false
                }).is_none() {
                    children.push(TreeNode::Node {id: child_id, content})
                }
            }
            TreeNode::Node {  .. } => {}
        }
    }
    fn to_data(&self) -> TreeNodeData {
        match self {
            TreeNode::Root { content, .. } => {TreeNodeData{
                id: None,
                hash: None, content: content.clone()
            }}
            TreeNode::Node { id, content, .. } => {TreeNodeData {
                id: Some(id.clone()),
                // todo
                hash: None, content: content.clone()
            }}
        }
    }
    fn total(&self) -> usize {
        match self {
            TreeNode::Root { children, .. } => {
                children.iter().fold(1, |mut total, x| {
                    total += x.total();
                    total
                })
            }
            TreeNode::Node { .. } => {1}
        }
    }

    fn get_children(
        &self,
        min: usize,
        max: usize,
    ) -> Vec<(usize, usize, TreeNodeData)> {
        match self {
            TreeNode::Root { children, .. } => {
                let mut children_data = Vec::new();
                if min == 0 {
                    children_data.push((0, 0, self.to_data()));
                }
                for (mut index, child) in children.iter().enumerate() {
                    index += 1;
                    if min <= index  && index <= max {
                        children_data.push((index, 1, child.to_data()));
                    }
                }
                children_data
            }
            TreeNode::Node { .. } => {vec![]}
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
    ) -> impl Iterator<Item = (usize, usize, TreeNodeData)> {
        let min = range.start;
        let max = range.end;
        let children = self.get_children(min, max);
        children.into_iter()
    }
}
