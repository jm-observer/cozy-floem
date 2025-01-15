use anyhow::{Result, anyhow};
use doc::{
    hit_position_aff,
    lines::{layout::*, line_ending::LineEnding, word::WordCursor}
};
use floem::{
    Clipboard, ViewId,
    kurbo::{Point, Rect, Size},
    peniko::Color,
    pointer::{PointerInputEvent, PointerMoveEvent},
    prelude::{SignalGet, SignalUpdate},
    reactive::RwSignal,
    text::{
        Attrs, AttrsList, FONT_SYSTEM, FamilyOwned, LineHeightValue
    }
};
use lapce_xi_rope::Rope;
use log::{debug, error, info};
use std::{borrow::Cow, cmp::Ordering, ops::Range};

#[derive(Copy, Clone, Debug)]
pub enum Position {
    Region { start: usize, end: usize },
    Caret(usize),
    None
}

#[derive(Clone, Debug)]
pub struct Cursor {
    dragging:     bool,
    pub position: Position
}

impl Cursor {
    pub fn offset(&self) -> Option<usize> {
        Some(match self.position {
            Position::Region { end, .. } => end,
            Position::Caret(offset) => offset,
            Position::None => return None
        })
    }

    pub fn start(&self) -> Option<usize> {
        Some(match self.position {
            Position::Region { start, .. } => start,
            Position::Caret(offset) => offset,
            Position::None => return None
        })
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

#[derive(Clone, Debug)]
pub struct DocStyle {
    pub font_family:  String,
    pub font_size:    f32,
    pub line_height:  f64,
    pub selection_bg: Color,
    pub fg_color:     Color
}

impl DocStyle {
    pub fn attrs<'a>(&self, family: &'a [FamilyOwned]) -> Attrs<'a> {
        Attrs::new()
            .family(&family)
            .font_size(self.font_size)
            .line_height(LineHeightValue::Px(self.line_height as f32))
    }
}

impl Default for DocStyle {
    fn default() -> Self {
        Self {
            font_family:  "JetBrains Mono".to_string(),
            font_size:    13.0,
            line_height:  23.0,
            selection_bg: Color::BLUE_VIOLET,
            fg_color:     Color::BLACK
        }
    }
}

pub struct SimpleDoc {
    pub id:                ViewId,
    pub rope:              Rope,
    pub visual_line:       Vec<VisualLine>,
    pub line_ending:       LineEnding,
    pub viewport:          Rect,
    pub cursor:            Cursor,
    pub hyperlink_regions: Vec<(Rect, Hyperlink)>,
    pub hover_hyperlink:   RwSignal<Option<usize>>,
    pub style:             DocStyle,
    pub auto_scroll:       bool
}

impl SimpleDoc {
    pub fn new(
        id: ViewId,
        hover_hyperlink: RwSignal<Option<usize>>
    ) -> Self {
        Self {
            id,
            rope: "".into(),
            visual_line: vec![],
            line_ending: LineEnding::Lf,
            viewport: Default::default(),
            cursor: Cursor {
                dragging: false,
                position: Position::None
            },
            hyperlink_regions: vec![],
            hover_hyperlink,
            style: Default::default(),
            auto_scroll: true
        }
    }

    pub fn pointer_down(
        &mut self,
        event: PointerInputEvent
    ) -> Result<()> {
        match event.count {
            1 => {
                if let Some(link) =
                    self.hover_hyperlink.get_untracked()
                {
                    if let Some(link) =
                        self.hyperlink_regions.get(link)
                    {
                        info!("todo {:?}", link.1);
                        // return Ok(())
                    }
                }
                let offset = self.offset_of_pos(event.pos)?.0;
                self.cursor.dragging = true;
                if event.modifiers.shift() {
                    self.cursor.position = Position::Region {
                        start: self.cursor.start().unwrap_or(offset),
                        end:   offset
                    };
                } else {
                    self.cursor.position = Position::Caret(offset);
                }
                self.id.request_paint();
            },
            2 => {
                let offset = self.offset_of_pos(event.pos)?.0;
                let (start_code, end_code) =
                    WordCursor::new(&self.rope, offset).select_word();
                self.cursor.position = Position::Region {
                    start: start_code,
                    end:   end_code
                };
                self.id.request_paint();
            },
            _ => {
                let line = self.offset_of_pos(event.pos)?.1;
                let offset = self.rope.offset_of_line(line)?;
                let next_line_offset =
                    self.rope.offset_of_line(line + 1)?;
                self.cursor.position = Position::Region {
                    start: offset,
                    end:   next_line_offset
                };
                self.id.request_paint();
            }
        }
        Ok(())
    }

    pub fn pointer_move(
        &mut self,
        event: PointerMoveEvent
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
                start: self.cursor.start().unwrap_or(offset),
                end:   offset
            };
            self.id.request_paint();
        }
        Ok(())
    }

    pub fn pointer_up(
        &mut self,
        _event: PointerInputEvent
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
        let Some(offset) = self.cursor.offset() else {
            return Ok(None);
        };
        let Some((point, _line, _)) = self.point_of_offset(offset)?
        else {
            return Ok(None);
        };
        let rect = Rect::from_origin_size(
            (point.x - 1.0, point.y),
            (2.0, self.style.line_height)
        );
        Ok(Some(rect))
    }

    fn point_of_offset(
        &self,
        offset: usize
    ) -> Result<Option<(Point, usize, usize)>> {
        if self.rope.is_empty() {
            return Ok(None);
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
        line as f64 * self.style.line_height
    }

    pub fn select_of_cursor(&self) -> Result<Vec<Rect>> {
        let Some((start_offset, end_offset)) = self.cursor.region()
        else {
            return Ok(vec![]);
        };
        let Some((start_point, mut start_line, _)) =
            self.point_of_offset(start_offset)?
        else {
            return Ok(vec![]);
        };
        let Some((mut end_point, end_line, _)) =
            self.point_of_offset(end_offset)?
        else {
            return Ok(vec![]);
        };
        end_point.y += self.style.line_height;
        if start_line == end_line {
            Ok(vec![Rect::from_points(start_point, end_point)])
        } else {
            let mut rects =
                Vec::with_capacity(end_line - start_line + 1);
            let viewport_width = self.viewport.width();
            rects.push(Rect::from_origin_size(
                start_point,
                (viewport_width, self.style.line_height)
            ));
            start_line += 1;
            while start_line < end_line {
                rects.push(Rect::from_origin_size(
                    Point::new(0.0, self.height_of_line(start_line)),
                    (viewport_width, self.style.line_height)
                ));
                start_line += 1;
            }
            rects.push(Rect::from_points(
                Point::new(0.0, self.height_of_line(start_line)),
                end_point
            ));
            Ok(rects)
        }
    }

    pub fn append_line(
        &mut self,
        Line {
            content,
            attrs_list,
            hyperlink
        }: Line
    ) {
        let len = self.rope.len();
        if len > 0 {
            self.rope.edit(len..len, self.line_ending.get_chars());
        }
        self.rope.edit(self.rope.len()..self.rope.len(), &content);
        let line_index = self.rope.line_of_offset(self.rope.len());
        let y =
            self.height_of_line(line_index) + self.style.line_height;
        let mut font_system = FONT_SYSTEM.lock();
        let text = TextLayout::new_with_font_system(
            line_index,
            content,
            attrs_list,
            &mut font_system
        );
        let points: Vec<(f64, f64, Hyperlink)> = hyperlink
            .into_iter()
            .map(|x| {
                let range = x.range();
                let x0 = text.hit_position(range.start).point.x;
                let x1 = text.hit_position(range.end).point.x;
                (x0, x1, x)
            })
            .collect();
        let hyperlinks: Vec<(Point, Point, Color)> = points
            .iter()
            .map(|(x0, x1, _link)| {
                (
                    Point::new(*x0, y - 1.0),
                    Point::new(*x1, y - 1.0),
                    self.style.fg_color
                )
            })
            .collect();
        let mut hyperlink_region: Vec<(Rect, Hyperlink)> = points
            .into_iter()
            .map(|(x0, x1, data)| {
                (
                    Rect::new(x0, y - self.style.line_height, x1, y),
                    data
                )
            })
            .collect();
        self.visual_line.push(VisualLine {
            pos_y: self.height_of_line(line_index),
            line_index,
            text_layout: TextLayoutLine { text, hyperlinks }
        });
        self.hyperlink_regions.append(&mut hyperlink_region);
        self.id.request_layout();
        self.id.request_paint();
        if self.auto_scroll {
            self.id.scroll_to(Some(Rect::from_origin_size(
                Point::new(
                    self.viewport.x0,
                    self.height_of_line(line_index)
                ),
                Size::new(
                    self.style.line_height,
                    self.style.line_height
                )
            )));
        }
    }

    pub fn append_lines<T: Styled>(
        &mut self,
        lines: T
    ) -> Result<()> {
        let mut old_len = self.rope.len();
        if old_len > 0 {
            if self.rope.byte_at(old_len - 1) != '\n' as u8 {
                self.rope.edit(
                    old_len..old_len,
                    self.line_ending.get_chars()
                );
                old_len += self.line_ending.len();
            }
        }
        self.rope
            .edit(self.rope.len()..self.rope.len(), lines.content());

        let old_line = self.rope.line_of_offset(old_len);
        let last_line = self.rope.line_of_offset(self.rope.len());
        let family = Cow::Owned(
            FamilyOwned::parse_list(&self.style.font_family)
                .collect()
        );
        debug!(
            "last_line={last_line} old_line={old_line} content={}",
            lines.content().len()
        );
        let mut delta = 0;
        let trim_str = ['\r', '\n'];
        for line_index in old_line..last_line {
            let start_offset =
                self.rope.offset_of_line(line_index)?;
            let end_offset =
                self.rope.offset_of_line(line_index + 1)?;
            let mut attrs_list =
                AttrsList::new(self.style.attrs(&family));
            let rang = start_offset - old_len..end_offset - old_len;
            let mut font_system = FONT_SYSTEM.lock();
            let content_origin =
                self.rope.slice_to_cow(start_offset..end_offset);
            let content = content_origin.trim_end_matches(&trim_str);
            // debug!("line_index={line_index} rang={rang:?}
            // content={content}");
            let hyperlink = lines.line_attrs(
                &mut attrs_list,
                self.style.attrs(&family),
                rang,
                delta
            );
            let text = TextLayout::new_with_font_system(
                line_index,
                content,
                attrs_list,
                &mut font_system
            );
            let points: Vec<(f64, f64, Hyperlink)> = hyperlink
                .into_iter()
                .map(|x| {
                    let range = x.range();
                    let x0 = text.hit_position(range.start).point.x;
                    let x1 = text.hit_position(range.end).point.x;
                    (x0, x1, x)
                })
                .collect();

            let y = self.height_of_line(line_index)
                + self.style.line_height;
            // let hyperlinks: Vec<(Point, Point, Color)> = vec![];
            let hyperlinks: Vec<(Point, Point, Color)> = points
                .iter()
                .map(|(x0, x1, _link)| {
                    (
                        Point::new(*x0, y - 1.0),
                        Point::new(*x1, y - 1.0),
                        self.style.fg_color
                    )
                })
                .collect();
            let mut hyperlink_region: Vec<(Rect, Hyperlink)> = points
                .into_iter()
                .map(|(x0, x1, data)| {
                    (
                        Rect::new(
                            x0,
                            y - self.style.line_height,
                            x1,
                            y
                        ),
                        data
                    )
                })
                .collect();
            self.visual_line.push(VisualLine {
                pos_y: self.height_of_line(line_index),
                line_index,
                text_layout: TextLayoutLine { text, hyperlinks }
            });
            self.hyperlink_regions.append(&mut hyperlink_region);
            delta += end_offset - start_offset;
        }

        self.id.request_layout();
        self.id.request_paint();
        if self.auto_scroll {
            self.id.scroll_to(Some(Rect::from_origin_size(
                Point::new(
                    self.viewport.x0,
                    self.height_of_line(
                        self.rope.line_of_offset(self.rope.len())
                    )
                ),
                Size::new(
                    self.style.line_height,
                    self.style.line_height
                )
            )));
        }
        Ok(())
    }

    /// return (offset_of_buffer, line)
    pub fn offset_of_pos(
        &self,
        point: Point
    ) -> Result<(usize, usize)> {
        let line = ((point.y / self.style.line_height) as usize)
            .min(self.visual_line.len() - 1);
        let text_layout = &self
            .visual_line
            .get(line)
            .ok_or(anyhow!("not found {} line", line))?
            .text_layout;
        let hit_point =
            text_layout.text.hit_point(Point::new(point.x, 0.0));
        debug!(
            "offset_of_pos point={point:?} line={line} index={} \
             self.visual_line.len()={}",
            hit_point.index,
            self.visual_line.len()
        );
        Ok((self.rope.offset_of_line(line)? + hit_point.index, line))
    }

    pub fn view_size(&self) -> Size {
        let viewport_size = self.viewport.size();
        let height = (self.visual_line.len() as f64
            * self.style.line_height
            + self.viewport.size().height / 4.0)
            .max(viewport_size.height);
        let max_width = self
            .viewport_lines()
            .iter()
            .fold(0., |x, line| {
                let width = line.text_layout.text.size().width;
                if x < width { width } else { x }
            })
            .max(self.viewport.size().width);
        Size::new(max_width, height)
    }

    pub fn viewport_lines(&self) -> &[VisualLine] {
        let len = self.visual_line.len().max(1) - 1;
        let min_line = ((self.viewport.y0 / self.style.line_height)
            .floor() as usize)
            .min(len);
        let max_line = ((self.viewport.y1 / self.style.line_height)
            .round() as usize)
            .min(self.visual_line.len());
        &self.visual_line[min_line..max_line]
    }

    pub fn update_viewport_by_scroll(&mut self, viewport: Rect) {
        let viewport_size = viewport.size();
        // viewport_size.height -= self.style.line_height / 0.5;
        // viewport_size.width -= self.style.line_height * 1.5;
        self.viewport = viewport.with_size(viewport_size);
        // info!("update_viewport_by_scroll {:?} {:?}",
        // viewport.size(), self.viewport.size());
        self.id.request_layout();
    }
}

#[derive(Clone)]
pub struct VisualLine {
    pub pos_y:       f64,
    pub line_index:  usize,
    pub text_layout: TextLayoutLine
}

#[derive(Clone, Debug)]
pub enum Hyperlink {
    File {
        range:  Range<usize>,
        src:    String,
        line:   usize,
        column: Option<usize>
    },
    Url {
        range: Range<usize>,
        // todo
        url:   String
    }
}
impl Hyperlink {
    pub fn range(&self) -> Range<usize> {
        match self {
            Hyperlink::File { range, .. } => range.clone(),
            Hyperlink::Url { range, .. } => range.clone()
        }
    }

    pub fn range_mut(&mut self, new_range: Range<usize>) {
        match self {
            Hyperlink::File { range, .. } => {
                *range = new_range;
            },
            Hyperlink::Url { range, .. } => {
                *range = new_range;
            }
        }
    }
}

#[derive(Clone)]
pub struct TextLayoutLine {
    pub hyperlinks: Vec<(Point, Point, Color)>,
    pub text:       TextLayout
}

#[derive(Clone)]
pub struct Line {
    pub content:    String,
    pub attrs_list: AttrsList,
    pub hyperlink:  Vec<Hyperlink>
}

pub fn ranges_overlap(
    r1: &Range<usize>,
    r2: &Range<usize>
) -> Option<Range<usize>> {
    let overlap = if r2.start <= r1.start && r1.start < r2.end {
        r1.start..r1.end.min(r2.end)
    } else if r1.start <= r2.start && r2.start < r1.end {
        r2.start..r2.end.min(r1.end)
    } else {
        return None;
    };
    if overlap.is_empty() {
        None
    } else {
        Some(overlap)
    }
}

pub trait Styled {
    fn content(&self) -> &str;
    fn line_attrs(
        &self,
        attrs: &mut AttrsList,
        default_attrs: Attrs,
        range: Range<usize>,
        delta: usize
    ) -> Vec<Hyperlink>;
}
