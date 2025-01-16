use anyhow::{Result, anyhow};
use doc::{
    hit_position_aff,
    lines::{layout::*, line_ending::LineEnding, word::WordCursor},
};
use floem::{
    Clipboard, ViewId,
    kurbo::{Point, Rect, Size},
    peniko::Color,
    pointer::{PointerInputEvent, PointerMoveEvent},
    prelude::{SignalGet, SignalUpdate},
    reactive::RwSignal,
    text::{
        Attrs, AttrsList, FONT_SYSTEM, FamilyOwned, LineHeightValue,
    },
};
use lapce_xi_rope::Rope;
use log::{debug, error, info};
use std::{borrow::Cow, cmp::Ordering, ops::Range};
use std::collections::HashMap;
use cargo_metadata::PackageId;
use floem::text::{Style, Weight};
use ansi_to_style::TextStyle;

#[derive(Copy, Clone, Debug)]
pub enum Position {
    Region { start: usize, end: usize },
    Caret(usize),
    None,
}

#[derive(Clone, Debug)]
pub struct Cursor {
    dragging: bool,
    pub position: Position,
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
    pub font_family: String,
    pub font_size: f32,
    pub line_height: f64,
    pub selection_bg: Color,
    pub fg_color: Color,
}

impl DocStyle {
    pub fn attrs<'a>(&self, family: &'a [FamilyOwned]) -> Attrs<'a> {
        Attrs::new()
            .family(family)
            .font_size(self.font_size)
            .line_height(LineHeightValue::Px(self.line_height as f32))
    }
}

impl Default for DocStyle {
    fn default() -> Self {
        Self {
            font_family: "JetBrains Mono".to_string(),
            font_size: 13.0,
            line_height: 23.0,
            selection_bg: Color::BLUE_VIOLET,
            fg_color: Color::BLACK,
        }
    }
}

pub struct SimpleDoc {
    pub id: ViewId,
    // pub visual_line:       Vec<VisualLine>,
    pub line_ending: LineEnding,
    pub viewport: Rect,
    pub cursor: Cursor,
    pub hyperlink_regions: Vec<(Rect, Hyperlink)>,
    pub hover_hyperlink: RwSignal<Option<usize>>,
    pub style: DocStyle,
    pub auto_scroll: bool,
    pub lines: Lines,
}

impl SimpleDoc {
    pub fn new(
        id: ViewId,
        hover_hyperlink: RwSignal<Option<usize>>,
    ) -> Self {
        Self {
            id,
            // visual_line: vec![],
            line_ending: LineEnding::Lf,
            viewport: Default::default(),
            cursor: Cursor {
                dragging: false,
                position: Position::None,
            },
            hyperlink_regions: vec![],
            hover_hyperlink,
            style: Default::default(),
            auto_scroll: true,
            lines: Default::default(),
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
                        info!("todo {:?}", link.1);
                        // return Ok(())
                    }
                }
                let offset = self.offset_of_pos(event.pos)?.0;
                self.cursor.dragging = true;
                if event.modifiers.shift() {
                    self.cursor.position = Position::Region {
                        start: self.cursor.start().unwrap_or(offset),
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
                    WordCursor::new(self.lines.rope(), offset).select_word();
                self.cursor.position = Position::Region {
                    start: start_code,
                    end: end_code,
                };
                self.id.request_paint();
            }
            _ => {
                let line = self.offset_of_pos(event.pos)?.1;
                let offset = self.offset_of_line(line)?;
                let next_line_offset =
                    self.offset_of_line(line + 1)?;
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
                start: self.cursor.start().unwrap_or(offset),
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
                self.lines.rope().slice_to_cow(start..end).to_string();
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
        // debug!("position_of_cursor offset={offset}, point={point:?}, line={_line}");
        let rect = Rect::from_origin_size(
            (point.x - 1.0, point.y),
            (2.0, self.style.line_height),
        );
        Ok(Some(rect))
    }

    fn point_of_offset(
        &self,
        offset: usize,
    ) -> Result<Option<(Point, usize, usize)>> {
        if self.lines.rope().is_empty() {
            return Ok(None);
        }
        let line = self.line_of_offset(offset);
        let offset_line = self.offset_of_line(line)?;
        let text = &self
            .lines.line(line)
            .ok_or(anyhow!("not found visual line: {line}"))?
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
                (viewport_width, self.style.line_height),
            ));
            start_line += 1;
            while start_line < end_line {
                rects.push(Rect::from_origin_size(
                    Point::new(0.0, self.height_of_line(start_line)),
                    (viewport_width, self.style.line_height),
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

    // pub fn append_line(
    //     &mut self,
    //     Line {
    //         content,
    //         attrs_list,
    //         hyperlink
    //     }: Line
    // ) {
    //     let len = self.rope.len();
    //     if len > 0 {
    //         self.rope.edit(len..len, self.line_ending.get_chars());
    //     }
    //     self.rope.edit(self.rope.len()..self.rope.len(), &content);
    //     let line_index = self.line_of_offset(self.rope.len());
    //     let y =
    //         self.height_of_line(line_index) + self.style.line_height;
    //     let mut font_system = FONT_SYSTEM.lock();
    //     let text = TextLayout::new_with_font_system(
    //         line_index,
    //         content,
    //         attrs_list,
    //         &mut font_system
    //     );
    //     let points: Vec<(f64, f64, Hyperlink)> = hyperlink
    //         .into_iter()
    //         .map(|x| {
    //             let range = x.range();
    //             let x0 = text.hit_position(range.start).point.x;
    //             let x1 = text.hit_position(range.end).point.x;
    //             (x0, x1, x)
    //         })
    //         .collect();
    //     let hyperlinks: Vec<(Point, Point, Color)> = points
    //         .iter()
    //         .map(|(x0, x1, _link)| {
    //             (
    //                 Point::new(*x0, y - 1.0),
    //                 Point::new(*x1, y - 1.0),
    //                 self.style.fg_color
    //             )
    //         })
    //         .collect();
    //     let mut hyperlink_region: Vec<(Rect, Hyperlink)> = points
    //         .into_iter()
    //         .map(|(x0, x1, data)| {
    //             (
    //                 Rect::new(x0, y - self.style.line_height, x1, y),
    //                 data
    //             )
    //         })
    //         .collect();
    //     self.visual_line.push(VisualLine {
    //         pos_y: self.height_of_line(line_index),
    //         line_index,
    //         text_layout: TextLayoutLine { text, hyperlinks },
    //         text_src: TextSrc::StdErr { level: ErrLevel::None },
    //     });
    //     self.hyperlink_regions.append(&mut hyperlink_region);
    //     self.id.request_layout();
    //     self.id.request_paint();
    //     if self.auto_scroll {
    //         self.id.scroll_to(Some(Rect::from_origin_size(
    //             Point::new(
    //                 self.viewport.x0,
    //                 self.height_of_line(line_index)
    //             ),
    //             Size::new(
    //                 self.style.line_height,
    //                 self.style.line_height
    //             )
    //         )));
    //     }
    // }
    //
    // pub fn append_lines<T: Styled>(
    //     &mut self,
    //     lines: T
    // ) -> Result<()> {
    //     let mut old_len = self.rope.len();
    //     if old_len > 0 && self.rope.byte_at(old_len - 1) != '\n' as u8 {
    //             self.rope.edit(
    //                 old_len..old_len,
    //                 self.line_ending.get_chars()
    //             );
    //             old_len += self.line_ending.len();
    //     }
    //     self.rope
    //         .edit(self.rope.len()..self.rope.len(), lines.content());
    //
    //     let old_line = self.line_of_offset(old_len);
    //     let mut last_line = self.line_of_offset(self.rope.len());
    //     // 新内容如果没有\n则会导致二者相等
    //     if last_line == old_line {
    //         last_line += 1;
    //     }
    //     let family = Cow::Owned(
    //         FamilyOwned::parse_list(&self.style.font_family)
    //             .collect()
    //     );
    //     // debug!(
    //     //     "last_line={last_line} old_line={old_line} content={}",
    //     //     lines.content().len()
    //     // );
    //     let mut delta = 0;
    //     let trim_str = ['\r', '\n'];
    //     let text_src = lines.src();
    //     for line_index in old_line..last_line {
    //         let start_offset =
    //             self.offset_of_line(line_index)?;
    //         let end_offset =
    //             self.offset_of_line(line_index + 1)?;
    //         let mut attrs_list =
    //             AttrsList::new(self.style.attrs(&family));
    //         let rang = start_offset - old_len..end_offset - old_len;
    //         let mut font_system = FONT_SYSTEM.lock();
    //         let content_origin =
    //             self.rope.slice_to_cow(start_offset..end_offset);
    //         let content = content_origin.trim_end_matches(&trim_str);
    //         // debug!("line_index={line_index} rang={rang:?}
    //         // content={content}");
    //         let hyperlink = lines.line_attrs(
    //             &mut attrs_list,
    //             self.style.attrs(&family),
    //             rang,
    //             delta
    //         );
    //         let text = TextLayout::new_with_font_system(
    //             line_index,
    //             content,
    //             attrs_list,
    //             &mut font_system
    //         );
    //         let points: Vec<(f64, f64, Hyperlink)> = hyperlink
    //             .into_iter()
    //             .map(|x| {
    //                 let range = x.range();
    //                 let x0 = text.hit_position(range.start).point.x;
    //                 let x1 = text.hit_position(range.end).point.x;
    //                 (x0, x1, x)
    //             })
    //             .collect();
    //
    //         let y = self.height_of_line(line_index)
    //             + self.style.line_height;
    //         // let hyperlinks: Vec<(Point, Point, Color)> = vec![];
    //         let hyperlinks: Vec<(Point, Point, Color)> = points
    //             .iter()
    //             .map(|(x0, x1, _link)| {
    //                 (
    //                     Point::new(*x0, y - 1.0),
    //                     Point::new(*x1, y - 1.0),
    //                     self.style.fg_color
    //                 )
    //             })
    //             .collect();
    //         let mut hyperlink_region: Vec<(Rect, Hyperlink)> = points
    //             .into_iter()
    //             .map(|(x0, x1, data)| {
    //                 (
    //                     Rect::new(
    //                         x0,
    //                         y - self.style.line_height,
    //                         x1,
    //                         y
    //                     ),
    //                     data
    //                 )
    //             })
    //             .collect();
    //         self.visual_line.push(VisualLine {
    //             pos_y: self.height_of_line(line_index),
    //             line_index,
    //             text_layout: TextLayoutLine { text, hyperlinks },
    //             text_src: text_src.clone(),
    //         });
    //         self.hyperlink_regions.append(&mut hyperlink_region);
    //         delta += end_offset - start_offset;
    //     }
    //
    //     self.id.request_layout();
    //     self.id.request_paint();
    //     if self.auto_scroll {
    //         self.id.scroll_to(Some(Rect::from_origin_size(
    //             Point::new(
    //                 self.viewport.x0,
    //                 self.height_of_line(
    //                     self.line_of_offset(self.rope.len())
    //                 )
    //             ),
    //             Size::new(
    //                 self.style.line_height,
    //                 self.style.line_height
    //             )
    //         )));
    //     }
    //     Ok(())
    // }

    pub fn append_lines(
        &mut self,
        lines: StyledText,
    ) -> Result<()> {
        let lines = lines.to_lines()?;
        self.lines.append_lines(lines, self.line_ending, &self.style)?;

        self.id.request_layout();
        self.id.request_paint();
        if self.auto_scroll {
            self.id.scroll_to(Some(Rect::from_origin_size(
                Point::new(
                    self.viewport.x0,
                    self.height_of_line(
                        self.line_of_offset(self.lines.rope().len())
                    ),
                ),
                Size::new(
                    self.style.line_height,
                    self.style.line_height,
                ),
            )));
        }
        Ok(())
    }

    /// return (offset_of_buffer, line)
    pub fn offset_of_pos(
        &self,
        point: Point,
    ) -> Result<(usize, usize)> {
        let line = ((point.y / self.style.line_height) as usize)
            .min(self.lines.lines_len() - 1);
        let text = &self
            .lines.line(line)
            .ok_or(anyhow!("not found visual line: {line}"))?
            .text;

        let hit_point =
            text.hit_point(Point::new(point.x, 0.0));
        debug!(
            "offset_of_pos point={point:?} line={line} index={} \
             self.visual_line.len()={}",
            hit_point.index,
            self.lines.lines_len()
        );
        Ok((self.offset_of_line(line)? + hit_point.index, line))
    }

    fn offset_of_line(&self, line: usize) -> Result<usize> {
        self.lines.rope().offset_of_line(line)
    }
    fn line_of_offset(&self, offset: usize) -> usize {
        self.lines.rope().line_of_offset(offset)
    }


    pub fn view_size(&self) -> Size {
        self.lines.visual_lines_size(self.viewport, self.style.line_height)
        // let viewport_size = self.viewport.size();
        // let height = (self.visual_line.len() as f64
        //     * self.style.line_height
        //     + self.viewport.size().height / 4.0)
        //     .max(viewport_size.height);
        // let max_width = self
        //     .viewport_lines()
        //     .iter()
        //     .fold(0., |x, line| {
        //         let width = line.text_layout.text.size().width;
        //         if x < width { width } else { x }
        //     })
        //     .max(self.viewport.size().width);
        // Size::new(max_width, height)
    }

    pub fn viewport_lines(&self) -> Vec<NewVisualLine> {
        self.lines.visual_lines(self.viewport, self.style.line_height)
        // let len = self.visual_line.len().max(1) - 1;
        // let min_line = ((self.viewport.y0 / self.style.line_height)
        //     .floor() as usize)
        //     .min(len);
        // let max_line = ((self.viewport.y1 / self.style.line_height)
        //     .round() as usize)
        //     .min(self.visual_line.len());
        // &self.visual_line[min_line..max_line]
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

#[derive(Clone, Debug)]
pub struct VisualLine {
    pub pos_y: f64,
    pub line_index: usize,
    pub text_layout: TextLayoutLine,
    pub text_src: TextSrc,
}

#[derive(Clone, Debug)]
pub enum Hyperlink {
    File {
        range: Range<usize>,
        src: String,
        line: usize,
        column: Option<usize>,
    },
    Url {
        range: Range<usize>,
        // todo
        url: String,
    },
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
            }
            Hyperlink::Url { range, .. } => {
                *range = new_range;
            }
        }
    }
}

#[derive(Clone, Debug)]
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

pub fn ranges_overlap(
    r1: &Range<usize>,
    r2: &Range<usize>,
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
    fn src(&self) -> TextSrc;
    fn content(&self) -> &str;
    fn line_attrs(
        &self,
        attrs: &mut AttrsList,
        default_attrs: Attrs,
        range: Range<usize>,
        delta: usize,
    ) -> Vec<Hyperlink>;
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TextSrc {
    StdOut {
        package_id: PackageId
    },
    StdErr {
        level: ErrLevel
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ErrLevel {
    None,
    Error,
}

#[derive(Debug, Clone, Default)]
pub enum DisplayStrategy {
    #[default]
    Viewport,
    TextSrc(TextSrc),
}

#[derive(Clone)]
pub struct StyledLines {
    pub text_src: TextSrc,
    pub lines: Vec<(String, Vec<TextStyle>, Vec<Hyperlink>)>,
}

#[derive(Debug, Clone, Default)]
pub struct Lines {
    pub rope: Rope,
    pub display_strategy: DisplayStrategy,
    pub ropes: HashMap<TextSrc, Rope>,
    pub visual_line: Vec<NewVisualLine>,
}

#[derive(Clone, Debug)]
pub struct NewVisualLine {
    pub pos_y: f64,
    pub line_index: usize,
    // pub text_layout: TextLayoutLine,
    pub text_src: TextSrc,
    pub hyperlink_regions: Vec<(Rect, Hyperlink)>,
    pub hyperlinks: Vec<(Point, Point, Color)>,
    pub text: TextLayout,
}

impl Lines {
    pub fn display_all(&mut self) {
        self.display_strategy = DisplayStrategy::Viewport
    }
    pub fn display_src(&mut self, text_src: TextSrc) {
        self.display_strategy = DisplayStrategy::TextSrc(text_src);
    }

    pub fn rope(&self) -> &Rope {
        match &self.display_strategy {
            DisplayStrategy::Viewport => { &self.rope }
            DisplayStrategy::TextSrc(src) => {
                self.ropes.get(src).unwrap_or(&self.rope)
            }
        }
    }

    fn display_by_viewport(&self, viewport: Rect, line_height: f64) -> &[NewVisualLine] {
        let len = self.visual_line.len().max(1) - 1;
        let min_line = ((viewport.y0 / line_height)
            .floor() as usize)
            .min(len);
        let max_line = ((viewport.y1 / line_height)
            .round() as usize)
            .min(self.visual_line.len());
        &self.visual_line[min_line..max_line]
    }

    pub fn line(&self, line: usize) -> Option<&NewVisualLine> {
        match &self.display_strategy {
            DisplayStrategy::Viewport => {
                self.visual_line.get(line)
            }
            DisplayStrategy::TextSrc(text_src) => {
                self.visual_line.iter().filter(|x| x.text_src == *text_src).nth(line)
            }
        }
    }
    pub fn lines_len(&self) -> usize {
        match &self.display_strategy {
            DisplayStrategy::Viewport => {
                self.visual_line.len()
            }
            DisplayStrategy::TextSrc(text_src) => {
                self.visual_line.iter().filter(|x| x.text_src == *text_src).count()
            }
        }
    }

    pub fn visual_lines(&self, viewport: Rect, line_height: f64) -> Vec<NewVisualLine> {
        match &self.display_strategy {
            DisplayStrategy::Viewport => {
                self.display_by_viewport(viewport, line_height).to_vec()
            }
            DisplayStrategy::TextSrc(text_src) => {
                self.visual_line.iter().filter_map(|x| if x.text_src == *text_src {
                    Some(x.clone())
                } else {
                    None
                }).collect()
            }
        }
    }

    fn visual_lines_size(&self, viewport: Rect, line_height: f64) -> Size {
        match &self.display_strategy {
            DisplayStrategy::Viewport => {
                let viewport_size = viewport.size();
                let height = (self.visual_line.len() as f64
                    * line_height
                    + viewport.size().height / 4.0)
                    .max(viewport_size.height);
                let max_width = self
                    .display_by_viewport(viewport, line_height)
                    .iter()
                    .fold(0., |x, line| {
                        let width = line.text.size().width;
                        if x < width { width } else { x }
                    })
                    .max(viewport.size().width);
                Size::new(max_width, height)
            }
            DisplayStrategy::TextSrc(text_src) => {
                let (len, max_width) = self.visual_line.iter().filter_map(|x| if x.text_src == *text_src {
                    Some(x.text.size().width)
                } else {
                    None
                }).fold((0., 0.), |(count, max_width), line_width| {
                    (count + 1., if max_width < line_width { line_width } else { max_width })
                });

                let viewport_size = viewport.size();
                let height = (len
                    * line_height
                    + viewport.size().height / 4.0)
                    .max(viewport_size.height);
                let max_width = max_width
                    .max(viewport.size().width);
                Size::new(max_width, height)
            }
        }
    }


    pub fn append_lines(
        &mut self,
        style_lines: StyledLines, line_ending: LineEnding, doc_style: &DocStyle,
    ) -> Result<()> {
        // 新内容如果没有\n则会导致二者相等
        let family = Cow::Owned(
            FamilyOwned::parse_list(&doc_style.font_family)
                .collect()
        );
        let mut old_len = self.rope.len();
        let mut line_index = if old_len > 0 {
            self.rope.line_of_offset(old_len)
        } else {
            0
        };

        for (content_origin_without_lf, style, hyperlink) in style_lines.lines.into_iter() {
            {
                self.rope.edit(old_len..old_len, &content_origin_without_lf);
                old_len += content_origin_without_lf.len();
                self.rope.edit(old_len..old_len, line_ending.get_chars());
                old_len += line_ending.len();
            }
            let mut attrs_list =
                AttrsList::new(doc_style.attrs(&family));
            style.into_iter().for_each(|x| to_line_attrs(&mut attrs_list, doc_style.attrs(&family), x));
            let mut font_system = FONT_SYSTEM.lock();
            let text = TextLayout::new_with_font_system(
                line_index,
                content_origin_without_lf,
                attrs_list,
                &mut font_system,
            );
            let y = line_index as f64 * doc_style.line_height;
            let points: Vec<(f64, f64, Hyperlink)> = hyperlink
                .into_iter()
                .map(|x| {
                    let range = x.range();
                    let x0 = text.hit_position(range.start).point.x;
                    let x1 = text.hit_position(range.end).point.x;
                    (x0, x1, x)
                })
                .collect();

            // let hyperlinks: Vec<(Point, Point, Color)> = vec![];
            let hyperlinks: Vec<(Point, Point, Color)> = points
                .iter()
                .map(|(x0, x1, _link)| {
                    (
                        Point::new(*x0, y + doc_style.line_height - 1.0),
                        Point::new(*x1, y + doc_style.line_height - 1.0),
                        doc_style.fg_color
                    )
                })
                .collect();
            let hyperlink_regions: Vec<(Rect, Hyperlink)> = points
                .into_iter()
                .map(|(x0, x1, data)| {
                    (
                        Rect::new(
                            x0,
                            y,
                            x1,
                            y + doc_style.line_height,
                        ),
                        data
                    )
                })
                .collect();
            let _line = NewVisualLine {
                pos_y: y,
                line_index,
                text,
                hyperlinks,
                text_src: style_lines.text_src.clone(),
                hyperlink_regions,
            };
            self.visual_line.push(_line);
            line_index += 1;
        }
        Ok(())
    }
}

fn to_line_attrs(
    attrs_list: &mut AttrsList,
    default_attrs: Attrs,
    x: TextStyle,
) {
    let TextStyle {
        range,
        bold,
        italic,
        fg_color,
        ..
    } = x;
    let mut attrs = default_attrs;
    if bold {
        attrs = attrs.weight(Weight::BOLD);
    }
    if italic {
        attrs = attrs.style(Style::Italic);
    }
    if let Some(fg) = fg_color {
        attrs = attrs.color(fg);
    }
    attrs_list.add_span(range, attrs);
}


#[derive(Clone)]
pub struct StyledText {
    pub text_src: TextSrc,
    pub styled_text: ansi_to_style::StyledText,
    pub hyperlink: Vec<Hyperlink>,
}

impl StyledText {
    pub fn to_lines(self) -> Result<StyledLines> {
        let rope: Rope = self.styled_text.text.into();
        let last_line = rope.line_of_offset(rope.len()) + 1;
        if last_line > 1 {
            error!("last_line={} {} {:?} {:?}", last_line, rope.to_string(), self.styled_text.styles, self.hyperlink)
        }
        let trim_str = ['\r', '\n'];
        //styles: Vec<(String, Vec<TextStyle>, Vec<Hyperlink>)>,
        let mut lines = Vec::with_capacity(last_line);
        for line in 0..last_line {
            let start_offset = rope.offset_of_line(line)?;
            let end_offset = rope.offset_of_line(line + 1)?;
            if start_offset == end_offset {
                break;
            }
            let content_origin =
                rope.slice_to_cow(start_offset..end_offset);
            let content = content_origin.trim_end_matches(&trim_str);
            let range = start_offset..start_offset + content.len();
            let links = self.hyperlink.iter().filter_map(|x| {
                if let Some(delta_range) =
                    ranges_overlap(&x.range(), &range)
                {
                    let mut link = x.clone();
                    let delta_range =delta_range.start - start_offset..delta_range.end - start_offset;
                    link.range_mut(delta_range);
                    Some(link)
                } else {
                    None
                }
            }).collect();

            let styles = self.styled_text.styles.iter().filter_map(|x| {
                ranges_overlap(&x.range, &range).map(|delta_range|
                {
                    TextStyle {
                        range: delta_range.start - start_offset..delta_range.end - start_offset,
                        bold: x.bold,
                        italic: x.italic,
                        underline: x.underline,
                        bg_color: x.bg_color,
                        fg_color: x.fg_color,
                    }
                })
            }).collect();
            if last_line > 1 {
                error!("[{content}] {:?} {:?}", styles, links)
            }

            lines.push((content.to_string(), styles, links));
        }
        Ok(StyledLines {
            text_src: self.text_src,
            lines,
        })
    }
}


impl crate::data::Styled for StyledText {
    fn src(&self) -> TextSrc {
        self.text_src.clone()
    }

    fn content(&self) -> &str {
        &self.styled_text.text
    }

    fn line_attrs(
        &self,
        attrs_list: &mut AttrsList,
        default_attrs: Attrs,
        range: Range<usize>,
        delta: usize,
    ) -> Vec<Hyperlink> {
        self.styled_text.styles.iter().for_each(|x| {
            if let Some(delta_range) =
                ranges_overlap(&x.range, &range)
            {
                let TextStyle {
                    bold,
                    italic,
                    fg_color,
                    ..
                } = x;
                let mut attrs = default_attrs;
                if *bold {
                    attrs = attrs.weight(Weight::BOLD);
                }
                if *italic {
                    attrs = attrs.style(Style::Italic);
                }
                if let Some(fg) = fg_color {
                    attrs = attrs.color(*fg);
                }
                let range = delta_range.start - delta
                    ..delta_range.end - delta;
                // debug!("delta_range={range:?}, style: {x:?}");
                attrs_list.add_span(range, attrs);
            }
        });
        self.hyperlink
            .iter()
            .filter_map(|x| {
                if let Some(delta_range) =
                    ranges_overlap(&x.range(), &range)
                {
                    let range = delta_range.start - delta
                        ..delta_range.end - delta;
                    let mut x = x.clone();
                    x.range_mut(range);
                    Some(x)
                } else {
                    None
                }
            })
            .collect()
    }
}