use std::ops::AddAssign;
use floem::peniko::Color;
use floem::prelude::{RwSignal, SignalGet, SignalUpdate, VirtualVector};
use floem::reactive::{batch, Scope};
use log::debug;
use crate::views::tree_with_panel::data::lines::{DisplayId};
use crate::views::tree_with_panel::data::tree::Level::Error;

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

#[derive(Clone, Debug, Copy)]
#[repr(u8)]
pub enum Level {
    None,
    Warn,
    Error,
}

impl Level {
    pub fn update(&mut self, level: Level) {
        debug!("{:?} {level:?}={}", self, level as u8);
        if level as u8 > *self as u8 {
            *self = level
        }
        debug!("after {:?}", self);
        // use Level::*;
        // if matches!(self, ref level) {
        //     return;
        // }
        // let new_level = match (level, &self) {
        //     (Error, Warn) | (_, None) => level,
        //      _ => return,
        // };
        // *self = new_level;
    }
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

    pub fn add_child(&mut self, id: DisplayId, level: Level) {
        batch(|| self._add_child(id, level));
    }
    fn _add_child(&mut self, id: DisplayId, level: Level) {
        // debug!("add_child {:?}", id);
        match &id {
            DisplayId::All => {}
            DisplayId::Error | DisplayId::Crate { .. } => {
                self.level.update(|x| x.update(level));
                if let Some(item) = self.children.iter_mut().find(|x| id == x.display_id) {
                    item.level.update(|x| x.update(level));
                } else {
                    self.children.push(TreeNode { cx: self.cx, display_id: id, level: self.cx.create_rw_signal(level), open: self.cx.create_rw_signal(true), children: vec![] })
                }
            }
            DisplayId::CrateFile { crate_name, .. } => {
                let crate_id = DisplayId::Crate { crate_name: crate_name.clone() };
                self.add_child(crate_id.clone(), level);
                if let Some(carte_item) = self.children.iter_mut().find(|x| crate_id == x.display_id) {
                    if let Some(item) = carte_item.children.iter_mut().find(|x| id == x.display_id) {
                        item.level.update(|x| x.update(level));
                    } else {
                        carte_item.children.push(TreeNode { cx: self.cx, display_id: id, level: self.cx.create_rw_signal(level), open: self.cx.create_rw_signal(false), children: vec![] })
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
        // debug!("min={min} max={max} {:?}", children);
        children.into_iter()
    }
}
