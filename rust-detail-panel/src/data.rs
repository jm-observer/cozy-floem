use std::borrow::Cow;
use lapce_xi_rope::{Interval, Rope};
use doc::lines::layout::*;
use doc::lines::line_ending::LineEnding;
use floem::kurbo::{Point, Rect};
use floem::peniko::Color;
use floem::pointer::{PointerInputEvent, PointerMoveEvent};
use floem::style::LineHeight;
use floem::text::{Attrs, AttrsList, FamilyOwned, FONT_SYSTEM, LineHeightValue, Weight};
use anyhow::{anyhow, Result};
use doc::hit_position_aff;
use floem::reactive::{create_trigger, Trigger};
use log::info;

#[derive(Copy, Clone, Debug)]
pub enum Position {
    Region {
        start: usize,
        end: usize,
    },
    Caret(usize),
}

#[derive(Clone, Debug)]
pub struct Cursor {
    dragging: bool,
    pub position: Position,
}

impl Cursor {
    pub fn offset(&self) -> usize {
        match self.position {
            Position::Region { end, .. } => { end }
            Position::Caret(offset) => { offset }
        }
    }

    pub fn start(&self) -> usize {
        match self.position {
            Position::Region { start, .. } => { start }
            Position::Caret(offset) => { offset }
        }
    }

    // pub fn dragging(&mut self) {
    //     self.dragging = true;
    // }
    //
    // pub fn dragged(&mut self) {
    //     self.dragging = false
    // }

    // pub fn update_offset(&mut self, offset: usize) {
    //     if self.dragging {
    //         self.position = Position::Region { start: self.start(), end: offset };
    //     } else {
    //         self.position = Position::Caret(offset)
    //     }
    // }
}

pub struct SimpleDoc {
    pub rope: Rope,
    pub visual_line: Vec<VisualLine>,
    pub line_ending: LineEnding,
    pub viewport: Rect,
    pub line_height: f64,
    pub cursor: Cursor,
    pub repaint: Trigger,
}

impl SimpleDoc {
    pub fn new(line_ending: LineEnding) -> Self {
        Self {
            rope: "".into(),
            visual_line: vec![],
            line_ending,
            viewport: Default::default(),
            line_height: 23.0,
            cursor: Cursor {
                dragging: false,
                position: Position::Caret(0),
            },
            repaint: create_trigger()
        }
    }

    pub fn pointer_down(&mut self, event: PointerInputEvent) -> Result<()> {
        let offset = self.offset_of_pos(event.pos)?;
        self.cursor.dragging = true;
        if event.modifiers.shift() {
            self.cursor.position = Position::Region { start: self.cursor.start(), end: offset };
        } else {
            self.cursor.position = Position::Caret(offset);
        }
        self.repaint.notify();
        info!("{:?} {:?}", event.pos, self.cursor);
        Ok(())
    }
    pub fn pointer_move(&mut self, event: PointerMoveEvent) -> Result<()> {
        if self.cursor.dragging {
            let offset = self.offset_of_pos(event.pos)?;
            self.cursor.position = Position::Region { start: self.cursor.start(), end: offset };
            self.repaint.notify();
        }
        Ok(())
    }

    pub fn pointer_up(&mut self, event: PointerInputEvent) -> Result<()> {
        self.cursor.dragging = false;
        let offset = self.offset_of_pos(event.pos)?;
        if offset != self.cursor.start() {
            self.cursor.position = Position::Region { start: self.cursor.start(), end: offset };
            self.repaint.notify();
        }
        info!("{:?} {:?}", event.pos, self.cursor);
        Ok(())
    }

    pub fn position_of_cursor(&self) -> Result<Rect> {
        let offset = self.cursor.offset();
        let line = self.rope.line_of_offset(offset);
        let offset_line = self.rope.offset_of_line(line)?;
        let text = &self.visual_line.get(line).ok_or(anyhow!("not found visual line: {line}"))?.text_layout.text;
        let point = hit_position_aff(text, offset - offset_line, true).point;
        let rect = Rect::from_origin_size((point.x - 1.0, point.y + (line as f64 - 0.5)* self.line_height), (2.0, self.line_height));
        Ok(rect)
    }

    pub fn append_line(&mut self, content: &str, attrs_list: AttrsList, extra_style: Vec<LineExtraStyle>) {
        let len = self.rope.len();
        if len > 0 {
            self.rope.edit(len..len, self.line_ending.get_chars());
        }
        self.rope.edit(self.rope.len()..self.rope.len(), content);
        let line_index = self.rope.line_of_offset(self.rope.len());
        let mut font_system = FONT_SYSTEM.lock();
        let text = TextLayout::new_with_font_system(line_index, content, attrs_list, &mut font_system);
        self.visual_line.push(VisualLine {
            line_index,
            text_layout: TextLayoutLine { text, extra_style },
        });
        self.repaint.notify();
    }

    pub fn offset_of_pos(&self, point: Point) -> Result<usize> {
        let line = (point.y / self.line_height) as usize;
        let text_layout = &self.visual_line.get(line).ok_or(anyhow!("not found {} line", line))?.text_layout;
        let hit_point = text_layout.text.hit_point(Point::new(point.x, 0.0));
        Ok(self.rope.offset_of_line(line)? + hit_point.index)
    }
}


#[derive(Clone)]
pub struct VisualLine {
    pub line_index: usize,
    pub text_layout: TextLayoutLine,
}

#[derive(Clone)]
pub struct TextLayoutLine {
    pub extra_style: Vec<LineExtraStyle>,
    pub text: TextLayout,
}

pub(crate) fn init_content(doc: &mut SimpleDoc, i: usize) {
    let content = format!("{} {}", "   Compiling icu_collections v1.5.0", i);
    let family =
        Cow::Owned(FamilyOwned::parse_list("JetBrains Mono").collect());
    let font_size = 13.0;
    let attrs = Attrs::new()
        // .color(self.editor_style.ed_text_color())
        .family(&family)
        .font_size(font_size as f32)
        .line_height(LineHeightValue::Px(23.0));
    let mut attr_list = AttrsList::new(attrs);
    let attrs = Attrs::new()
        .color(Color::GREEN)
        .family(&family)
        .font_size(font_size as f32).weight(Weight::BOLD)
        .line_height(LineHeightValue::Px(23.0));
    attr_list.add_span(3..12, attrs);
    doc.append_line(&content, attr_list, vec![]);
}