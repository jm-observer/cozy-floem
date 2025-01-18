use std::ops::AddAssign;
use floem::peniko::Color;
use floem::prelude::{RwSignal, SignalGet, VirtualVector};
use floem::reactive::Scope;
use log::debug;
use crate::views::tree_with_panel::data::lines::{DisplayId};

#[derive(Clone)]
pub struct TreeNode {
    pub cx: Scope,
    pub display_id: DisplayId,
    pub children: Vec<TreeNode>,
    pub level: RwSignal<Level>,
    pub open: RwSignal<bool>,
}

#[derive(Clone, Debug)]
pub struct TreeNodeData {
    pub display_id: DisplayId,
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
                r#"<svg width="16" height="16" viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="currentColor"><path fill-rule="evenodd" clip-rule="evenodd" d="M7.56 1h.88l6.54 12.26-.44.74H1.44L1 13.26 7.56 1zM8 2.28L2.28 13H13.7L8 2.28zM8.625 12v-1h-1.25v1h1.25zm-1.25-2V6h1.25v4h-1.25z"/></svg>"#
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
    pub fn add_child(&mut self, id: DisplayId) {
        debug!("add_child {:?}", id);
        match &id {
            DisplayId::All => {}
            DisplayId::Error | DisplayId::Crate { .. } => {
                if self.children.iter().find(|x| id == x.display_id).is_none() {
                    self.children.push(TreeNode { cx: self.cx, display_id: id, level: self.cx.create_rw_signal(Level::None), open: self.cx.create_rw_signal(true), children: vec![] })
                }
            }
            DisplayId::CrateFile { crate_name, .. } => {
                let crate_id = DisplayId::Crate { crate_name: crate_name.clone() };
                self.add_child(crate_id.clone());
                if let Some(carte_item) = self.children.iter_mut().find(|x| crate_id == x.display_id) {
                    if carte_item.children.iter().find(|x| id == x.display_id).is_none() {
                        carte_item.children.push(TreeNode { cx: self.cx, display_id: id, level: self.cx.create_rw_signal(Level::None), open: self.cx.create_rw_signal(false), children: vec![] })
                    }
                }
            }
        }
    }
    fn to_data(&self) -> TreeNodeData {
        TreeNodeData {
            display_id: self.display_id.clone(),
            open: self.open,
            level: self.level,
        }
    }
    fn total(&self) -> usize {
        if self.open.get() {
            self.children.iter().fold(1, |mut total, x| {
                total += x.total();
                total
            })
        } else {
            1
        }
    }

    fn get_children(
        &self,
        min: usize,
        max: usize, index: &mut usize, level: usize,
    ) -> Vec<(usize, usize, TreeNodeData)> {
        let mut children_data = Vec::new();
        if min <= *index && *index <= max {
            children_data.push((*index, level, self.to_data()));
        } else {
            return children_data;
        }
        index.add_assign(1);
        if self.open.get() {
            for child in self.children.iter() {
                let mut children = child.get_children(min, max, index, level + 1);
                children_data.append(&mut children);
            }
        }
        children_data
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
        let mut index = 0;
        let children = self.get_children(min, max, &mut index, 0);
        debug!("min={min} max={max} {:?}", children);
        children.into_iter()
    }
}
