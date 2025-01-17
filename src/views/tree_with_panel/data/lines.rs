use std::ops::Range;
use floem::kurbo::{Point, Rect, Size};
use lapce_xi_rope::Rope;
use std::collections::HashMap;
use doc::lines::layout::TextLayout;
use log::{error, warn};
use floem::peniko::Color;
use anyhow::anyhow;
use doc::hit_position_aff;
use doc::lines::line_ending::LineEnding;
use std::borrow::Cow;
use floem::text::{Attrs, AttrsList, FamilyOwned, FONT_SYSTEM, Style, Weight};
use ansi_to_style::TextStyle;
use cargo_metadata::PackageId;
use crate::views::tree_with_panel::data::{StyledLines, VisualLine};
use crate::views::tree_with_panel::data::panel::DocStyle;

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

#[derive(Debug, Clone, Default)]
pub struct Lines {
    pub rope:             Rope,
    pub display_strategy: DisplayStrategy,
    pub ropes: HashMap<
        TextSrc,
        (Rope, Vec<SimpleLine>, Vec<SimpleHyperlink>)
    >,
    pub visual_line:      Vec<SimpleLine>,
    pub visual_links:     Vec<SimpleHyperlink>,
    pub hyperlinks:       Vec<Hyperlink>,
    pub texts:            Vec<TextLayout>
}

#[derive(Clone, Debug)]
pub struct SimpleHyperlink {
    pub rect:       Rect,
    pub link_index: usize
}

impl SimpleHyperlink {
    pub fn underline(&self) -> (Point, Point) {
        (
            Point::new(self.rect.x0, self.rect.y1 - 2.0),
            Point::new(self.rect.x1, self.rect.y1 - 2.0)
        )
    }
}

#[derive(Clone, Debug)]
pub struct SimpleLine {
    pub line_index: usize,
    pub hyperlinks: Vec<(Point, Point)>,
    pub text_index: usize
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
            DisplayStrategy::Viewport => &self.rope,
            DisplayStrategy::TextSrc(src) => self
                .ropes
                .get(src)
                .map(|(rope, _lines, _link)| rope)
                .unwrap_or(&self.rope)
        }
    }

    fn display_simple_lines(
        &self,
        viewport: Rect,
        line_height: f64
    ) -> &[SimpleLine] {
        let lines = self.line_info().1;
        let len = lines.len().max(1) - 1;
        let min_line =
            ((viewport.y0 / line_height).floor() as usize).min(len);
        let max_line = ((viewport.y1 / line_height).round() as usize)
            .min(lines.len());
        &lines[min_line..max_line]
    }

    fn line_info(
        &self
    ) -> (&Rope, &[SimpleLine], &[SimpleHyperlink]) {
        match &self.display_strategy {
            DisplayStrategy::Viewport => {
                (&self.rope, &self.visual_line, &self.visual_links)
            },
            DisplayStrategy::TextSrc(text_src) => {
                let Some((rope, line, link)) =
                    self.ropes.get(text_src)
                else {
                    error!("not found {:?}", text_src);
                    return (
                        &self.rope,
                        &self.visual_line,
                        &self.visual_links
                    );
                };
                (rope, line, link)
            }
        }
    }

    pub fn lines_len(&self) -> usize {
        self.line_info().1.len()
    }

    pub fn visual_lines(
        &self,
        viewport: Rect,
        line_height: f64,
        hyper_color: Color
    ) -> Vec<VisualLine> {
        self.display_simple_lines(viewport, line_height)
            .iter()
            .filter_map(|x| {
                let pos_y: f64 = x.line_index as f64 * line_height;

                let hyperlinks = x
                    .hyperlinks
                    .clone()
                    .into_iter()
                    .map(|x| (x.0, x.1, hyper_color))
                    .collect();
                if let Some(text) = self.texts.get(x.text_index) {
                    Some(VisualLine {
                        pos_y,
                        line_index: x.line_index,
                        hyperlinks,
                        text: text.clone()
                    })
                } else {
                    warn!("not found text layout: {}", x.text_index);
                    None
                }
            })
            .collect()
    }

    pub(crate) fn visual_lines_size(
        &self,
        viewport: Rect,
        line_height: f64
    ) -> Size {
        let viewport_size = viewport.size();
        let len = self.lines_len();
        let height = (len as f64 * line_height
            + viewport.size().height / 4.0)
            .max(viewport_size.height);

        let max_width = self
            .display_simple_lines(viewport, line_height)
            .iter()
            .fold(0., |x, line| {
                let Some(text_layout) =
                    self.text_layout_of_line(line.line_index)
                else {
                    warn!(
                        "not found text layout {}",
                        line.line_index
                    );
                    return x;
                };
                let width = text_layout.size().width;
                if x < width { width } else { x }
            })
            .max(viewport.size().width);
        Size::new(max_width, height)
    }

    pub fn text_layout_of_line(
        &self,
        line: usize
    ) -> Option<&TextLayout> {
        let line_index = self.line_info().1.get(line);
        line_index.and_then(|index| self.texts.get(index.text_index))
    }

    pub fn point_of_offset(
        &self,
        offset: usize
    ) -> anyhow::Result<Option<(Point, usize, usize)>> {
        let rope = self.rope();
        if rope.is_empty() {
            return Ok(None);
        }
        let offset = offset.min(rope.len() - 1);
        let line = rope.line_of_offset(offset);
        let offset_line = rope.offset_of_line(line)?;
        let text = self
            .text_layout_of_line(line)
            .ok_or(anyhow!("not found visual line: {line}"))?;
        let point =
            hit_position_aff(text, offset - offset_line, true).point;
        Ok(Some((point, line, offset_line)))
    }

    #[allow(clippy::too_many_arguments)]
    fn push_src(
        &mut self,
        text_src: &TextSrc,
        content_origin_without_lf: String,
        text_index: usize,
        hyperlink: &[Hyperlink],
        line_ending: LineEnding,
        text: &TextLayout,
        line_height: f64
    ) {
        let (rope, lines, links) =
            self.ropes.entry(text_src.clone()).or_default();
        let mut old_len = rope.len();
        let line_index = if old_len > 0 {
            rope.line_of_offset(old_len)
        } else {
            0
        };
        {
            rope.edit(old_len..old_len, &content_origin_without_lf);
            old_len += content_origin_without_lf.len();
            rope.edit(old_len..old_len, line_ending.get_chars());
        }

        let start = links.len();
        let (underlines, mut simple_link): (
            Vec<(Point, Point)>,
            Vec<SimpleHyperlink>
        ) = hyperlink
            .iter()
            .map(|x| {
                let range = x.range();
                let x0 = text.hit_position(range.start).point.x;
                let x1 = text.hit_position(range.end).point.x;
                let y0 = line_index as f64 * line_height;
                let y1 = (line_index + 1) as f64 * line_height;
                let under_line_y =
                    (line_index + 1) as f64 * line_height - 2.0;
                (
                    Point::new(x0, under_line_y),
                    Point::new(x1, under_line_y),
                    Rect::new(x0, y0, x1, y1)
                )
            })
            .enumerate()
            .fold(
                (
                    Vec::with_capacity(hyperlink.len()),
                    Vec::with_capacity(hyperlink.len())
                ),
                |(mut underlines, mut simple_link), (index, x)| {
                    underlines.push((x.0, x.1));

                    simple_link.push(SimpleHyperlink {
                        rect:       x.2,
                        link_index: start + index
                    });
                    (underlines, simple_link)
                }
            );
        let _line = SimpleLine {
            line_index,
            text_index,
            hyperlinks: underlines
        };
        links.append(&mut simple_link);
        lines.push(_line);
    }

    pub fn in_hyperlink_region(
        &self,
        position: Point
    ) -> Option<usize> {
        let links = self.line_info().2;
        links.iter().find_map(|x| {
            if x.rect.contains(position) {
                Some(x.link_index)
            } else {
                None
            }
        })
    }

    pub fn hyperlink_by_point(
        &self,
        position: Point
    ) -> Option<&Hyperlink> {
        self.in_hyperlink_region(position).and_then(|x| {
            let rs = self.hyperlinks.get(x);
            if rs.is_none() {
                error!("not found hyperlink: {}", x);
            }
            rs
        })
    }

    pub fn append_lines(
        &mut self,
        style_lines: StyledLines,
        line_ending: LineEnding,
        doc_style: &DocStyle
    ) -> anyhow::Result<()> {
        // 新内容如果没有\n则会导致二者相等
        let family = Cow::Owned(
            FamilyOwned::parse_list(&doc_style.font_family).collect()
        );
        let mut old_len = self.rope.len();
        let mut line_index = if old_len > 0 {
            self.rope.line_of_offset(old_len)
        } else {
            0
        };
        let text_src = style_lines.text_src;
        for (content_origin_without_lf, style, mut hyperlink) in
            style_lines.lines.into_iter()
        {
            {
                self.rope.edit(
                    old_len..old_len,
                    &content_origin_without_lf
                );
                old_len += content_origin_without_lf.len();
                self.rope
                    .edit(old_len..old_len, line_ending.get_chars());
                old_len += line_ending.len();
            }
            let mut attrs_list =
                AttrsList::new(doc_style.attrs(&family));
            style.into_iter().for_each(|x| {
                to_line_attrs(
                    &mut attrs_list,
                    doc_style.attrs(&family),
                    x
                )
            });
            let mut font_system = FONT_SYSTEM.lock();
            let text = TextLayout::new_with_font_system(
                line_index,
                &content_origin_without_lf,
                attrs_list,
                &mut font_system
            );

            let text_index = self.texts.len();
            let start = self.hyperlinks.len();
            let (underlines, mut simple_link): (
                Vec<(Point, Point)>,
                Vec<SimpleHyperlink>
            ) = hyperlink
                .iter()
                .map(|x| {
                    let range = x.range();
                    let x0 = text.hit_position(range.start).point.x;
                    let x1 = text.hit_position(range.end).point.x;
                    let y0 =
                        line_index as f64 * doc_style.line_height;
                    let y1 = (line_index + 1) as f64
                        * doc_style.line_height;
                    let under_line_y = (line_index + 1) as f64
                        * doc_style.line_height
                        - 2.0;
                    (
                        Point::new(x0, under_line_y),
                        Point::new(x1, under_line_y),
                        Rect::new(x0, y0, x1, y1)
                    )
                })
                .enumerate()
                .fold(
                    (
                        Vec::with_capacity(hyperlink.len()),
                        Vec::with_capacity(hyperlink.len())
                    ),
                    |(mut underlines, mut simple_link),
                     (index, x)| {
                        underlines.push((x.0, x.1));

                        simple_link.push(SimpleHyperlink {
                            rect:       x.2,
                            link_index: start + index
                        });
                        (underlines, simple_link)
                    }
                );

            let _line = SimpleLine {
                line_index,
                text_index,
                hyperlinks: underlines
            };
            self.visual_line.push(_line);
            self.visual_links.append(&mut simple_link);
            if let Some(text_src) = &text_src {
                self.push_src(
                    text_src,
                    content_origin_without_lf,
                    text_index,
                    &hyperlink,
                    line_ending,
                    &text,
                    doc_style.line_height
                );
            }
            self.texts.push(text);
            self.hyperlinks.append(&mut hyperlink);
            // let hyperlinks: Vec<(Point, Point, Color)> = vec![];
            // let hyperlinks: Vec<(Point, Point, Color)> = points
            //     .iter()
            //     .map(|(x0, x1, _link)| {
            //         (
            //             Point::new(*x0, y + doc_style.line_height -
            // 1.0),             Point::new(*x1, y +
            // doc_style.line_height - 1.0),
            // doc_style.fg_color         )
            //     })
            //     .collect();
            // let hyperlink_regions: Vec<(Rect, Hyperlink)> = points
            //     .into_iter()
            //     .map(|(x0, x1, data)| {
            //         (
            //             Rect::new(
            //                 x0,
            //                 y,
            //                 x1,
            //                 y + doc_style.line_height,
            //             ),
            //             data
            //         )
            //     })
            //     .collect();
            line_index += 1;
        }
        Ok(())
    }
}

fn to_line_attrs(
    attrs_list: &mut AttrsList,
    default_attrs: Attrs,
    x: TextStyle
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

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TextSrc {
    StdOut { package_id: PackageId },
    StdErr { level: ErrLevel }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ErrLevel {
    Error
}

#[derive(Debug, Clone, Default)]
pub enum DisplayStrategy {
    #[default]
    Viewport,
    TextSrc(TextSrc)
}
