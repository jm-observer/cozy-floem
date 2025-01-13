use anyhow::{Result, anyhow};
use doc::{
    hit_position_aff,
    lines::{layout::*, line_ending::LineEnding, word::WordCursor},
};
use floem::{Clipboard, kurbo::{Point, Rect}, peniko::Color, pointer::{PointerInputEvent, PointerMoveEvent}, prelude::{SignalGet, SignalUpdate}, reactive::{RwSignal, Trigger, create_trigger}, text::{AttrsList, FONT_SYSTEM}, ViewId};
use lapce_xi_rope::Rope;
use log::{error, info};
use std::cmp::Ordering;

#[derive(Copy, Clone, Debug)]
pub enum Position {
    Region { start: usize, end: usize },
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
            Position::Region { end, .. } => end,
            Position::Caret(offset) => offset
        }
    }

    pub fn start(&self) -> usize {
        match self.position {
            Position::Region { start, .. } => start,
            Position::Caret(offset) => offset
        }
    }

    pub fn region(&self) -> Option<(usize, usize)> {
        if let Position::Region { start, end } = self.position {
            match start.cmp(&end) {
                Ordering::Less => Some((start, end)),
                Ordering::Equal => None,
                Ordering::Greater => Some((end, start))
            }
        } else {
            None
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct DocStyle {
    pub selection_bg: Color,
}

impl Default for DocStyle {
    fn default() -> Self {
        Self {
            selection_bg: Color::BLUE_VIOLET
        }
    }
}

pub struct SimpleDoc {
    pub id: ViewId,
    pub rope: Rope,
    pub visual_line: Vec<VisualLine>,
    pub line_ending: LineEnding,
    pub viewport: Rect,
    pub line_height: f64,
    pub cursor: Cursor,
    pub hyperlink_regions: Vec<(Rect, Hyperlink)>,
    pub hover_hyperlink: RwSignal<Option<usize>>,
    pub style: DocStyle,
}

impl SimpleDoc {
    pub fn new(id: ViewId,
        line_ending: LineEnding,
        hover_hyperlink: RwSignal<Option<usize>>,
    ) -> Self {
        Self {
            id,
            rope: "".into(),
            visual_line: vec![],
            line_ending,
            viewport: Default::default(),
            line_height: 23.0,
            cursor: Cursor {
                dragging: false,
                position: Position::Caret(0),
            },
            hyperlink_regions: vec![],
            hover_hyperlink,
            style: Default::default(),
        }
    }

    pub fn pointer_down(
        &mut self,
        event: PointerInputEvent,
    ) -> Result<()> {
        match event.count {
            1 => {
                if let Some(link) =
                    self.hover_hyperlink.get_untracked()
                {
                    if let Some(link) =
                        self.hyperlink_regions.get(link)
                    {
                        info!("todo {}", link.1.link);
                        // return Ok(())
                    }
                }
                let offset = self.offset_of_pos(event.pos)?.0;
                self.cursor.dragging = true;
                if event.modifiers.shift() {
                    self.cursor.position = Position::Region {
                        start: self.cursor.start(),
                        end: offset,
                    };
                } else {
                    self.cursor.position = Position::Caret(offset);
                }
                self.id.request_paint();
            }
            2 => {
                let offset = self.offset_of_pos(event.pos)?.0;
                let (start_code, end_code) =
                    WordCursor::new(&self.rope, offset).select_word();
                self.cursor.position = Position::Region {
                    start: start_code,
                    end: end_code,
                };
                self.id.request_paint();
            }
            _ => {
                let line = self.offset_of_pos(event.pos)?.1;
                let offset = self.rope.offset_of_line(line)?;
                let next_line_offset =
                    self.rope.offset_of_line(line + 1)?;
                self.cursor.position = Position::Region {
                    start: offset,
                    end: next_line_offset,
                };
                self.id.request_paint();
            }
        }
        Ok(())
    }

    pub fn pointer_move(
        &mut self,
        event: PointerMoveEvent,
    ) -> Result<()> {
        if let Some(x) =
            self.hyperlink_regions.iter().enumerate().find_map(
                |(index, x)| {
                    if x.0.contains(event.pos) {
                        Some(index)
                    } else {
                        None
                    }
                }
            )
        {
            if self.hover_hyperlink.get_untracked().is_none() {
                self.hover_hyperlink.set(Some(x));
            }
        } else if self.hover_hyperlink.get_untracked().is_some() {
            self.hover_hyperlink.set(None);
        }
        if self.cursor.dragging {
            let offset = self.offset_of_pos(event.pos)?.0;
            self.cursor.position = Position::Region {
                start: self.cursor.start(),
                end: offset,
            };
            self.id.request_paint();
        }
        Ok(())
    }

    pub fn pointer_up(
        &mut self,
        _event: PointerInputEvent,
    ) -> Result<()> {
        self.cursor.dragging = false;
        Ok(())
    }

    pub fn copy_select(&self) {
        if let Some((start, end)) = self.cursor.region() {
            let content =
                self.rope.slice_to_cow(start..end).to_string();
            if let Err(err) = Clipboard::set_contents(content) {
                error!("{err:?}");
            }
        }
    }

    pub fn position_of_cursor(&self) -> Result<Option<Rect>> {
        let offset = self.cursor.offset();
        let Some((point, _line, _)) = self.point_of_offset(offset)? else {
            return Ok(None)
        };
        let rect = Rect::from_origin_size(
            (point.x - 1.0, point.y),
            (2.0, self.line_height),
        );
        Ok(Some(rect))
    }

    fn point_of_offset(
        &self,
        offset: usize,
    ) -> Result<Option<(Point, usize, usize)>> {
        if self.rope.is_empty() {
            return Ok(None)
        }
        let line = self.rope.line_of_offset(offset);
        let offset_line = self.rope.offset_of_line(line)?;
        let text = &self
            .visual_line
            .get(line)
            .ok_or(anyhow!("not found visual line: {line}"))?
            .text_layout
            .text;
        let mut point =
            hit_position_aff(text, offset - offset_line, true).point;
        point.y = self.height_of_line(line);
        Ok(Some((point, line, offset_line)))
    }

    fn height_of_line(&self, line: usize) -> f64 {
        line as f64 * self.line_height
    }

    pub fn select_of_cursor(&self) -> Result<Vec<Rect>> {
        let Some((start_offset, end_offset)) = self.cursor.region()
        else {
            return Ok(vec![]);
        };
        let Some((start_point, mut start_line, _)) =
            self.point_of_offset(start_offset)? else {
            return Ok(vec![])
        };
        let Some((mut end_point, end_line, _)) =
            self.point_of_offset(end_offset)? else {
            return Ok(vec![])
        };
        end_point.y += self.line_height;
        if start_line == end_line {
            Ok(vec![Rect::from_points(start_point, end_point)])
        } else {
            let mut rects =
                Vec::with_capacity(end_line - start_line + 1);
            let viewport_width = self.viewport.width();
            rects.push(Rect::from_origin_size(
                start_point,
                (viewport_width, self.line_height),
            ));
            start_line += 1;
            while start_line < end_line {
                rects.push(Rect::from_origin_size(
                    Point::new(0.0, self.height_of_line(start_line)),
                    (viewport_width, self.line_height),
                ));
                start_line += 1;
            }
            rects.push(Rect::from_points(
                Point::new(0.0, self.height_of_line(start_line)),
                end_point,
            ));
            Ok(rects)
        }
    }

    pub fn append_line(
        &mut self,
        Line {
            content, attrs_list, hyperlink
        }: Line,
    ) {
        let len = self.rope.len();
        if len > 0 {
            self.rope.edit(len..len, self.line_ending.get_chars());
        }
        self.rope.edit(self.rope.len()..self.rope.len(), &content);
        let line_index = self.rope.line_of_offset(self.rope.len());
        let y = self.height_of_line(line_index) + self.line_height;
        let mut font_system = FONT_SYSTEM.lock();
        let text = TextLayout::new_with_font_system(
            line_index,
            content,
            attrs_list,
            &mut font_system,
        );
        let points: Vec<(f64, f64, Hyperlink)> = hyperlink
            .into_iter()
            .map(|x| {
                let x0 = text.hit_position(x.start_offset).point.x;
                let x1 = text.hit_position(x.end_offset).point.x;
                (x0, x1, x)
            })
            .collect();
        let hyperlinks: Vec<(Point, Point, Color)> = points
            .iter()
            .map(|(x0, x1, link)| {
                (
                    Point::new(*x0, y - 1.0),
                    Point::new(*x1, y - 1.0),
                    link.line_color
                )
            })
            .collect();
        let mut hyperlink_region: Vec<(Rect, Hyperlink)> = points
            .into_iter()
            .map(|(x0, x1, data)| {
                (Rect::new(x0, y - self.line_height, x1, y), data)
            })
            .collect();
        self.visual_line.push(VisualLine {
            line_index,
            text_layout: TextLayoutLine { text, hyperlinks },
        });
        self.hyperlink_regions.append(&mut hyperlink_region);
        self.id.request_layout();
        self.id.request_paint();
    }

    /// return (offset_of_buffer, line)
    pub fn offset_of_pos(
        &self,
        point: Point,
    ) -> Result<(usize, usize)> {
        let line = (point.y / self.line_height) as usize;
        let text_layout = &self
            .visual_line
            .get(line)
            .ok_or(anyhow!("not found {} line", line))?
            .text_layout;
        let hit_point =
            text_layout.text.hit_point(Point::new(point.x, 0.0));
        Ok((self.rope.offset_of_line(line)? + hit_point.index, line))
    }
}

#[derive(Clone)]
pub struct VisualLine {
    pub line_index: usize,
    pub text_layout: TextLayoutLine,
}
#[derive(Clone)]
pub struct Hyperlink {
    pub start_offset: usize,
    pub end_offset: usize,
    pub link: String,
    pub line_color: Color,
}

#[derive(Clone)]
pub struct TextLayoutLine {
    pub hyperlinks: Vec<(Point, Point, Color)>,
    pub text: TextLayout,
}

#[derive(Clone)]
pub struct Line {
    pub content: String,
    pub attrs_list: AttrsList,
    pub hyperlink: Vec<Hyperlink>,
}