pub mod cursor;
pub mod tree;
pub mod panel;
pub mod lines;

use ansi_to_style::TextStyle;
use anyhow::Result;
use doc::lines::layout::*;
use floem::{
    kurbo::Point,
    peniko::Color
};
use lapce_xi_rope::Rope;
use std::ops::Range;
use lines::{Hyperlink, TextSrc};

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

#[derive(Clone)]
pub struct StyledLines {
    pub text_src: TextSrc,
    pub lines:    Vec<(String, Vec<TextStyle>, Vec<Hyperlink>)>
}

#[derive(Clone, Debug)]
pub struct VisualLine {
    pub pos_y:      f64,
    pub line_index: usize,
    pub hyperlinks: Vec<(Point, Point, Color)>,
    pub text:       TextLayout
}

#[derive(Clone)]
pub struct StyledText {
    pub id:    TextSrc,
    pub styled_text: ansi_to_style::StyledText,
    pub hyperlink:   Vec<Hyperlink>,
}

impl StyledText {

    pub fn to_lines(self) -> Result<StyledLines> {
        let rope: Rope = self.styled_text.text.into();
        let last_line = rope.line_of_offset(rope.len()) + 1;
        // if last_line > 1 {
        //     error!("last_line={} {} {:?} {:?}", last_line,
        // rope.to_string(), self.styled_text.styles, self.hyperlink)
        // }
        let trim_str = ['\r', '\n'];
        //styles: Vec<(String, Vec<TextStyle>, Vec<Hyperlink>)>,
        let mut lines = Vec::with_capacity(last_line);
        for line in 0..last_line {
            let start_offset = rope.offset_of_line(line)?;
            let end_offset = rope.offset_of_line(line + 1)?;

            let content_origin =
                rope.slice_to_cow(start_offset..end_offset);
            let content = content_origin.trim_end_matches(&trim_str);
            if start_offset == end_offset || content.is_empty() {
                continue;
            }
            let range = start_offset..start_offset + content.len();
            let links = self
                .hyperlink
                .iter()
                .filter_map(|x| {
                    if let Some(delta_range) =
                        ranges_overlap(&x.range(), &range)
                    {
                        let mut link = x.clone();
                        let delta_range = delta_range.start
                            - start_offset
                            ..delta_range.end - start_offset;
                        link.range_mut(delta_range);
                        Some(link)
                    } else {
                        None
                    }
                })
                .collect();

            let styles = self
                .styled_text
                .styles
                .iter()
                .filter_map(|x| {
                    ranges_overlap(&x.range, &range).map(
                        |delta_range| TextStyle {
                            range:     delta_range.start
                                - start_offset
                                ..delta_range.end - start_offset,
                            bold:      x.bold,
                            italic:    x.italic,
                            underline: x.underline,
                            bg_color:  x.bg_color,
                            fg_color:  x.fg_color
                        }
                    )
                })
                .collect();
            // if last_line > 1 {
            //     error!("[{content}] {:?} {:?}", styles, links)
            // }

            lines.push((content.to_string(), styles, links));
        }
        Ok(StyledLines {
            text_src: self.id,
            lines
        })
    }
}

// impl crate::data::Styled for StyledText {
//     fn content(&self) -> &str {
//         &self.styled_text.text
//     }
//
//     fn line_attrs(
//         &self,
//         attrs_list: &mut AttrsList,
//         default_attrs: Attrs,
//         range: Range<usize>,
//         delta: usize
//     ) -> Vec<Hyperlink> {
//         self.styled_text.styles.iter().for_each(|x| {
//             if let Some(delta_range) =
//                 ranges_overlap(&x.range, &range)
//             {
//                 let TextStyle {
//                     bold,
//                     italic,
//                     fg_color,
//                     ..
//                 } = x;
//                 let mut attrs = default_attrs;
//                 if *bold {
//                     attrs = attrs.weight(Weight::BOLD);
//                 }
//                 if *italic {
//                     attrs = attrs.style(Style::Italic);
//                 }
//                 if let Some(fg) = fg_color {
//                     attrs = attrs.color(*fg);
//                 }
//                 let range = delta_range.start - delta
//                     ..delta_range.end - delta;
//                 // debug!("delta_range={range:?}, style: {x:?}");
//                 attrs_list.add_span(range, attrs);
//             }
//         });
//         self.hyperlink
//             .iter()
//             .filter_map(|x| {
//                 if let Some(delta_range) =
//                     ranges_overlap(&x.range(), &range)
//                 {
//                     let range = delta_range.start - delta
//                         ..delta_range.end - delta;
//                     let mut x = x.clone();
//                     x.range_mut(range);
//                     Some(x)
//                 } else {
//                     None
//                 }
//             })
//             .collect()
//     }
// }
