use std::{
    ops::{AddAssign, Range},
    sync::{Arc, atomic, atomic::AtomicUsize},
};
use std::borrow::Cow;
use std::fmt::{Debug, Formatter};

use anyhow::{Result, anyhow, bail};
use floem::{
    context::StyleCx,
    kurbo::{Point, Rect, Size},
    peniko::{Brush, Color},
    reactive::{
        ReadSignal, RwSignal, Scope, SignalGet, SignalUpdate, SignalWith, batch,
    },
    text::{Attrs, AttrsList, FONT_SYSTEM, Wrap},
};
use floem::text::FamilyOwned;
use itertools::Itertools;
use lapce_xi_rope::{Interval, Rope, RopeDelta, Transformer, spans::{Spans, SpansBuilder}};
use layout::{TextLayout, TextLayoutLine};
use line::{OriginFoldedLine, VisualLine};
use log::{debug, error, info, warn};
use lsp_types::{DiagnosticSeverity, InlayHint, InlayHintLabel, Location, Position};
use phantom_text::{
    PhantomText, PhantomTextKind, PhantomTextLine, PhantomTextMultiLine,
};
use signal::Signals;
use smallvec::SmallVec;
use style::NewLineStyle;

use crate::{
    DiagnosticData, EditorViewKind,
    config::EditorConfig,
    hit_position_aff,
    lines::{
        action::UpdateFolding,
        buffer::{Buffer, InvalLines, rope_text::RopeText},
        cursor::{ColPosition, Cursor, CursorAffinity, CursorMode},
        edit::{Action, EditConf, EditType},
        encoding::{offset_utf8_to_utf16, offset_utf16_to_utf8},
        fold::{FoldingDisplayItem, FoldingRanges},
        line::OriginLine,
        line_ending::LineEnding,
        phantom_text::Text,
        screen_lines::ScreenLines,
        selection::Selection,
        word::{CharClassification, WordCursor, get_char_property},
    },
    syntax::{BracketParser, Syntax, edit::SyntaxEdit},
};
use crate::lines::command::EditCommand;
use crate::lines::delta_compute::{OriginLinesDelta, resolve_delta_rs};
use crate::lines::indent::IndentStyle;
use crate::lines::mode::{Mode, MotionMode};
use crate::lines::register::Register;
use crate::lines::style::EditorStyle;
use crate::lines::text::{PreeditData, SystemClipboard, WrapMethod};

pub mod action;
pub mod buffer;
pub mod cursor;
pub mod diff;
pub mod edit;
pub mod encoding;
pub mod fold;
pub mod indent;
pub mod layout;
pub mod line;
pub mod line_ending;
pub mod paragraph;
pub mod phantom_text;
pub mod screen_lines;
pub mod selection;
pub mod signal;
pub mod style;
pub mod util;
pub mod word;
pub mod delta_compute;
pub mod movement;
pub mod command;
pub mod mode;
pub mod text;
pub mod char_buffer;
pub mod chars;
pub mod register;
pub mod editor_command;
pub mod soft_tab;

// /// Minimum width that we'll allow the view to be wrapped at.
// const MIN_WRAPPED_WIDTH: f32 = 100.0;

#[derive(Clone)]
pub struct LinesOfOriginOffset {
    pub origin_offset: usize,
    pub origin_line: OriginLine,
    pub origin_folded_line: OriginFoldedLine,
    // 在折叠行的偏移值
    pub origin_folded_line_offest: usize,
    pub visual_line: VisualLine,
    // 在视觉行的偏移值
    pub visual_line_offest: usize,
}

#[derive(Clone, Copy)]
pub struct DocLinesManager {
    lines: RwSignal<DocLines>,
}

impl DocLinesManager {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cx: Scope,
        diagnostics: DiagnosticData,
        syntax: Syntax,
        parser: BracketParser,
        viewport: Rect,
        editor_style: EditorStyle,
        config: EditorConfig,
        buffer: Buffer,
        kind: RwSignal<EditorViewKind>,
    ) -> Result<Self> {
        Ok(Self {
            lines: cx.create_rw_signal(DocLines::new(
                cx,
                diagnostics,
                syntax,
                parser,
                viewport,
                editor_style,
                config,
                buffer,
                kind,
            )?)
        })
    }

    pub fn with_untracked<O>(&self, f: impl FnOnce(&DocLines) -> O) -> O {
        self.lines.with_untracked(f)
    }

    pub fn get(&self) -> DocLines {
        self.lines.get()
    }

    pub fn update(&self, f: impl FnOnce(&mut DocLines)) {
        // not remove `batch`!
        batch(|| {
            self.lines.update(f);
        });
    }

    pub fn try_update<O>(&self, f: impl FnOnce(&mut DocLines) -> O) -> Option<O> {
        // not remove `batch`!
        batch(|| self.lines.try_update(f))
    }

    pub fn lines_of_origin_offset(
        &self,
        origin_offset: usize,
    ) -> Result<LinesOfOriginOffset> {
        self.with_untracked(|x| {
            let rs = x.lines_of_origin_offset(origin_offset);
            if rs.is_err() {
                x.log();
            }
            rs
        })
    }
}

#[derive(Clone)]
pub struct DocLines {
    // pub origin_lines: Vec<OriginLine>,
    pub origin_lines: Vec<OriginLine>,
    pub origin_folded_lines: Vec<OriginFoldedLine>,
    pub visual_lines: Vec<VisualLine>,
    // pub font_sizes: Rc<EditorFontSizes>,
    // font_size_cache_id: FontSizeCacheId,
    // wrap: ResolvedWrap,
    // pub layout_event: Listener<LayoutEvent>,
    max_width: f64,

    // editor: Editor
    pub inlay_hints: Option<Spans<InlayHint>>,
    pub completion_lens: Option<String>,
    pub completion_pos: (usize, usize),
    pub folding_ranges: FoldingRanges,
    // pub buffer: Buffer,
    pub diagnostics: DiagnosticData,

    /// Current inline completion text, if any.
    /// This will be displayed even on views that are not focused.
    /// (line, col)
    pub inline_completion: Option<(String, usize, usize)>,
    pub preedit: PreeditData,
    // tree-sitter
    pub syntax: Syntax,
    // lsp 来自lsp的语义样式.string是指代码的类别，如macro、function
    pub semantic_styles: Option<(Option<String>, Spans<String>)>,
    pub parser: BracketParser,
    // /// 用于存储每行的前景色样式。如keyword的颜色
    // pub line_styles: HashMap<usize, Vec<NewLineStyle>>,
    pub editor_style: EditorStyle,
    viewport_size: Size,
    pub config: EditorConfig,
    // pub buffer: Buffer,
    // pub buffer_rev: u64,
    pub kind: RwSignal<EditorViewKind>,
    pub(crate) signals: Signals,
    style_from_lsp: bool,
    // folding_items: Vec<FoldingDisplayItem>,
    pub line_height: usize, // pub screen_lines: ScreenLines,
}

impl DocLines {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cx: Scope,
        diagnostics: DiagnosticData,
        syntax: Syntax,
        parser: BracketParser,
        viewport: Rect,
        editor_style: EditorStyle,
        config: EditorConfig,
        buffer: Buffer,
        kind: RwSignal<EditorViewKind>,
    ) -> Result<Self> {
        let screen_lines = ScreenLines::new(cx, viewport, 0.0);
        let last_line = buffer.last_line() + 1;
        let signals = Signals::new(
            cx,
            &editor_style,
            viewport,
            buffer,
            screen_lines,
            (last_line, 0.0),
        );
        let mut lines = Self {
            signals,
            // layout_event: Listener::new_empty(cx), //
            // font_size_cache_id: id,
            viewport_size: viewport.size(),
            config,
            editor_style,
            origin_lines: vec![],
            origin_folded_lines: vec![],
            visual_lines: vec![],
            max_width: 0.0,

            inlay_hints: None,
            completion_pos: (0, 0),
            folding_ranges: Default::default(),
            // buffer: Buffer::new(""),
            diagnostics,
            completion_lens: None,
            inline_completion: None,
            preedit: PreeditData::new(cx),
            syntax,
            semantic_styles: None,
            parser,
            // line_styles: Default::default(),
            kind,
            style_from_lsp: false,
            // folding_items: Default::default(),
            line_height: 0,
        };
        lines.update_lines_new(OriginLinesDelta::default())?;
        Ok(lines)
    }

    // pub fn update_cache_id(&mut self) {
    //     let current_id = self.font_sizes.cache_id();
    //     if current_id != self.font_size_cache_id {
    //         self.font_size_cache_id = current_id;
    //         self.update()
    //     }
    // }

    // pub fn update_font_sizes(&mut self, font_sizes:
    // Rc<EditorFontSizes>) {     self.font_sizes = font_sizes;
    //     self.update()
    // }

    fn clear(&mut self) {
        self.max_width = 0.0;
        self.line_height = 0;
    }

    fn update_parser(&mut self) -> Result<()> {
        let buffer = self.signals.buffer.val(); // 提前保存，结束不可变借用
        let styles_exist = self.syntax.styles.is_some(); // 提前判断，不再借用 self.syntax

        let parser = &mut self.parser; // 现在安全地进行可变借用
        if styles_exist {
            parser.update_code(buffer, Some(&self.syntax))?;
        } else {
            parser.update_code(buffer, None)?;
        }
        Ok(())
    }

    // fn update_lines_old(&mut self) {
    //     self.clear();
    //
    //     let last_line = self.buffer.last_line();
    //     let semantic_styles = self.init_semantic_styes();
    //     // self.update_parser(buffer);
    //     let mut current_line = 0;
    //     let mut origin_folded_line_index = 0;
    //     let mut visual_line_index = 0;
    //     self.line_height = self.config.line_height;
    //
    //     let font_size = self.config.font_size;
    //     let family = Cow::Owned(
    //         FamilyOwned::parse_list(&self.config.font_family).
    // collect(),     );
    //     let attrs = Attrs::new()
    //         .color(self.editor_style.ed_text_color())
    //         .family(&family)
    //         .font_size(font_size as f32)
    //         .line_height(LineHeightValue::Px(self.line_height as
    // f32));     // let mut duration = Duration::from_secs(0);
    //     while current_line <= last_line {
    //         let start_offset =
    // self.buffer.offset_of_line(current_line);         let
    // end_offset = self.buffer.offset_of_line(current_line + 1);
    //         // let time = std::time::SystemTime::now();
    //         let text_layout = self.new_text_layout(
    //             current_line,
    //             start_offset,
    //             end_offset,
    //             font_size,
    //             attrs, &semantic_styles,
    //         );
    //         // duration += time.elapsed().unwrap();
    //         let origin_line_start = text_layout.phantom_text.line;
    //         let origin_line_end =
    // text_layout.phantom_text.last_line;
    //
    //         let width = text_layout.text.size().width;
    //         if width > self.max_width {
    //             self.max_width = width;
    //         }
    //
    //         for origin_line in origin_line_start..=origin_line_end
    // {             self.origin_lines.push(OriginLine {
    //                 line_index: origin_line,
    //                 start_offset,
    //                 phantom: Default::default(),
    //                 fg_styles: vec![],
    //             });
    //         }
    //
    //         let origin_interval = Interval {
    //             start:
    // self.buffer.offset_of_line(origin_line_start),
    // end: self.buffer.offset_of_line(origin_line_end + 1),
    //         };
    //
    //         let mut visual_offset_start = 0;
    //         let mut visual_offset_end;
    //
    //         // [visual_offset_start..visual_offset_end)
    //         for (origin_folded_line_sub_index, layout) in
    //             text_layout.text.line_layout().iter().enumerate()
    //         {
    //             if layout.glyphs.is_empty() {
    //                 self.visual_lines.push(VisualLine {
    //                     line_index: visual_line_index,
    //                     origin_interval: Interval::new(
    //                         origin_interval.end,
    //                         origin_interval.end,
    //                     ),
    //                     visual_interval: Interval::new(
    //                         visual_offset_start,
    //                         visual_offset_start,
    //                     ),
    //                     origin_line: origin_line_start,
    //                     origin_folded_line:
    // origin_folded_line_index,
    // origin_folded_line_sub_index: 0,
    // text_layout: text_layout.clone(),                 });
    //                 continue;
    //             }
    //             visual_offset_end = visual_offset_start +
    // layout.glyphs.len() - 1;             let offset_info =
    // text_layout                 .phantom_text
    //
    // .cursor_position_of_final_col(visual_offset_start);
    //             let origin_interval_start =
    //                 self.buffer.offset_of_line(offset_info.0) +
    // offset_info.1;             let offset_info = text_layout
    //                 .phantom_text
    //
    // .cursor_position_of_final_col(visual_offset_end);
    //
    //             let origin_interval_end =
    //                 self.buffer.offset_of_line(offset_info.0) +
    // offset_info.1;             let origin_interval = Interval {
    //                 start: origin_interval_start,
    //                 end: origin_interval_end + 1,
    //             };
    //
    //             self.visual_lines.push(VisualLine {
    //                 line_index: visual_line_index,
    //                 origin_interval,
    //                 origin_line: origin_line_start,
    //                 origin_folded_line: origin_folded_line_index,
    //                 origin_folded_line_sub_index,
    //                 text_layout: text_layout.clone(),
    //                 visual_interval: Interval::new(
    //                     visual_offset_start,
    //                     visual_offset_end + 1,
    //                 ),
    //             });
    //
    //             visual_offset_start = visual_offset_end;
    //             visual_line_index += 1;
    //         }
    //
    //         self.origin_folded_lines.push(OriginFoldedLine {
    //             line_index: origin_folded_line_index,
    //             origin_line_start,
    //             origin_line_end,
    //             origin_interval,
    //             text_layout,
    //         });
    //
    //         current_line = origin_line_end + 1;
    //         origin_folded_line_index += 1;
    //     }
    //     self.on_update_lines();
    // }

    // fn update_lines_2(&mut self, (_start_delta, _end_delta):
    // (Option<LineDelta>, Option<LineDelta>)) {     self.clear();
    //     self.origin_lines.clear();
    //     self.origin_folded_lines.clear();
    //     self.visual_lines.clear();
    //     let last_line = self.buffer().last_line();
    //     let mut current_line = 0;
    //     let mut origin_folded_line_index = 0;
    //     let mut visual_line_index = 0;
    //     self.line_height = self.config.line_height;
    //     let font_size = self.config.font_size;
    //     let family = Cow::Owned(
    //         FamilyOwned::parse_list(&self.config.font_family).
    // collect(),     );
    //     let attrs = Attrs::new()
    //         .color(self.editor_style.ed_text_color())
    //         .family(&family)
    //         .font_size(font_size as f32)
    //         .line_height(LineHeightValue::Px(self.line_height as
    // f32));     // let mut duration = Duration::from_secs(0);
    //
    //     let all_origin_lines = self.init_all_origin_line((&None,
    // &None));     while current_line <= last_line {
    //         let Some((text_layout, semantic_styles,
    // diagnostic_styles)) = self.new_text_layout_2(
    // current_line,             &all_origin_lines,
    //             font_size,
    //             attrs,
    //         ) else {
    //             // todo
    //             break;
    //         };
    //         // duration += time.elapsed().unwrap();
    //         let origin_line_start = text_layout.phantom_text.line;
    //         let origin_line_end =
    // text_layout.phantom_text.last_line;
    //
    //         let width = text_layout.text.size().width;
    //         if width > self.max_width {
    //             self.max_width = width;
    //         }
    //
    //         let origin_interval = Interval {
    //             start:
    // self.buffer().offset_of_line(origin_line_start),
    //             end: self.buffer().offset_of_line(origin_line_end +
    // 1),         };
    //
    //         let mut visual_offset_start = 0;
    //         let mut visual_offset_end;
    //
    //         // [visual_offset_start..visual_offset_end)
    //         for (origin_folded_line_sub_index, layout) in
    //             text_layout.text.line_layout().iter().enumerate()
    //         {
    //             if layout.glyphs.is_empty() {
    //                 self.visual_lines.push(VisualLine {
    //                     line_index: visual_line_index,
    //                     origin_interval: Interval::new(
    //                         origin_interval.end,
    //                         origin_interval.end,
    //                     ),
    //                     visual_interval: Interval::new(
    //                         visual_offset_start,
    //                         visual_offset_start,
    //                     ),
    //                     origin_line: origin_line_start,
    //                     origin_folded_line:
    // origin_folded_line_index,
    // origin_folded_line_sub_index: 0,                     //
    // text_layout: text_layout.clone(),                 });
    //                 continue;
    //             }
    //             visual_offset_end = visual_offset_start +
    // layout.glyphs.len() - 1;             let offset_info =
    // text_layout                 .phantom_text
    //
    // .cursor_position_of_final_col(visual_offset_start);
    //             let origin_interval_start =
    //                 self.buffer().offset_of_line(offset_info.0) +
    // offset_info.1;             let offset_info = text_layout
    //                 .phantom_text
    //
    // .cursor_position_of_final_col(visual_offset_end);
    //
    //             let origin_interval_end =
    //                 self.buffer().offset_of_line(offset_info.0) +
    // offset_info.1;             let origin_interval = Interval {
    //                 start: origin_interval_start,
    //                 end: origin_interval_end + 1,
    //             };
    //
    //             self.visual_lines.push(VisualLine {
    //                 line_index: visual_line_index,
    //                 origin_interval,
    //                 origin_line: origin_line_start,
    //                 origin_folded_line: origin_folded_line_index,
    //                 origin_folded_line_sub_index,
    //                 // text_layout: text_layout.clone(),
    //                 visual_interval: Interval::new(
    //                     visual_offset_start,
    //                     visual_offset_end + 1,
    //                 ),
    //             });
    //
    //             visual_offset_start = visual_offset_end;
    //             visual_line_index += 1;
    //         }
    //
    //         self.origin_folded_lines.push(OriginFoldedLine {
    //             line_index: origin_folded_line_index,
    //             origin_line_start,
    //             origin_line_end,
    //             origin_interval,
    //             text_layout,
    //             semantic_styles,
    //             diagnostic_styles,
    //         });
    //
    //         current_line = origin_line_end + 1;
    //         origin_folded_line_index += 1;
    //     }
    //     self.origin_lines = all_origin_lines;
    //     self.on_update_lines();
    // }

    // fn update_lines(
    //     &mut self,
    //     (start_delta, end_delta): (Option<LineDelta>, Option<LineDelta>)
    // ) -> Result<()> {
    //     self.clear();
    //     self.visual_lines.clear();
    //     self.line_height = self.config.line_height;
    //     let last_line = self.signals.buffer.val().last_line();
    //     let font_size = self.config.font_size;
    //     let family =
    //         Cow::Owned(FamilyOwned::parse_list(&self.config.font_family).collect());
    //     let attrs = Attrs::new()
    //         .color(self.editor_style.ed_text_color())
    //         .family(&family)
    //         .font_size(font_size as f32)
    //         .line_height(LineHeightValue::Px(self.line_height as f32));
    //     // let mut duration = Duration::from_secs(0);
    //
    //     let all_origin_lines =
    //         self.init_all_origin_line((&start_delta, &end_delta))?;
    //
    //     let mut origin_folded_lines = if let Some(LineDelta {
    //         start_line,
    //         end_line,
    //         ..
    //     }) = start_delta
    //     {
    //         self.origin_folded_lines
    //             .iter()
    //             .filter_map(|folded| {
    //                 if start_line <= folded.origin_line_start
    //                     && folded.origin_line_end < end_line
    //                 {
    //                     Some(folded.clone())
    //                 } else {
    //                     None
    //                 }
    //             })
    //             .collect()
    //     } else {
    //         Vec::new()
    //     };
    //     {
    //         let mut origin_folded_line_index = 0;
    //
    //         let mut current_line = if let Some(line) = origin_folded_lines.last() {
    //             line.origin_line_end + 1
    //         } else {
    //             0
    //         };
    //         while current_line <= last_line {
    //             let (text_layout, semantic_styles, diagnostic_styles) = self
    //                 .new_text_layout_2(
    //                     current_line,
    //                     &all_origin_lines,
    //                     font_size,
    //                     attrs
    //                 )?;
    //             // duration += time.elapsed().unwrap();
    //             let origin_line_start = text_layout.phantom_text.line;
    //             let origin_line_end = text_layout.phantom_text.last_line;
    //
    //             let width = text_layout.text.size().width;
    //             if width > self.max_width {
    //                 self.max_width = width;
    //             }
    //
    //             let origin_interval = Interval {
    //                 start: self.buffer().offset_of_line(origin_line_start)?,
    //                 end:   self.buffer().offset_of_line(origin_line_end + 1)?
    //             };
    //
    //             origin_folded_lines.push(OriginFoldedLine {
    //                 line_index: origin_folded_line_index,
    //                 origin_line_start,
    //                 origin_line_end,
    //                 origin_interval,
    //                 text_layout,
    //                 semantic_styles,
    //                 diagnostic_styles
    //             });
    //
    //             current_line = origin_line_end + 1;
    //             origin_folded_line_index += 1;
    //         }
    //     }
    //     {
    //         let mut visual_line_index = 0;
    //         // while let Some(line) = origin_line_iter.next() {
    //         for line in origin_folded_lines.iter() {
    //             // duration += time.elapsed().unwrap();
    //             let text_layout = &line.text_layout;
    //             let origin_line_start = text_layout.phantom_text.line;
    //             let origin_line_end = text_layout.phantom_text.last_line;
    //             let origin_folded_line_index = line.line_index;
    //
    //             let origin_interval = Interval {
    //                 start: self.buffer().offset_of_line(origin_line_start)?,
    //                 end:   self.buffer().offset_of_line(origin_line_end + 1)?
    //             };
    //
    //             let mut visual_offset_start = 0;
    //             let mut visual_offset_end;
    //
    //             // [visual_offset_start..visual_offset_end)
    //             for (origin_folded_line_sub_index, layout) in
    //                 text_layout.text.line_layout().iter().enumerate()
    //             {
    //                 if layout.glyphs.is_empty() {
    //                     self.visual_lines.push(VisualLine {
    //                         line_index:                   visual_line_index,
    //                         origin_interval:              Interval::new(
    //                             origin_interval.end,
    //                             origin_interval.end
    //                         ),
    //                         visual_interval:              Interval::new(
    //                             visual_offset_start,
    //                             visual_offset_start
    //                         ),
    //                         origin_line:                  origin_line_start,
    //                         origin_folded_line:           origin_folded_line_index,
    //                         origin_folded_line_sub_index: 0 /* text_layout:
    //                                                          * text_layout.
    //                                                          * clone(), */
    //                     });
    //                     continue;
    //                 }
    //                 visual_offset_end =
    //                     visual_offset_start + layout.glyphs.len() - 1;
    //                 let offset_info = text_layout
    //                     .phantom_text
    //                     .cursor_position_of_final_col(visual_offset_start);
    //                 let origin_interval_start =
    //                     self.buffer().offset_of_line(offset_info.0)? + offset_info.1;
    //                 let offset_info = text_layout
    //                     .phantom_text
    //                     .cursor_position_of_final_col(visual_offset_end);
    //
    //                 let origin_interval_end =
    //                     self.buffer().offset_of_line(offset_info.0)? + offset_info.1;
    //                 let origin_interval = Interval {
    //                     start: origin_interval_start,
    //                     end:   origin_interval_end + 1
    //                 };
    //
    //                 self.visual_lines.push(VisualLine {
    //                     line_index: visual_line_index,
    //                     origin_interval,
    //                     origin_line: origin_line_start,
    //                     origin_folded_line: origin_folded_line_index,
    //                     origin_folded_line_sub_index,
    //                     // text_layout: text_layout.clone(),
    //                     visual_interval: Interval::new(
    //                         visual_offset_start,
    //                         visual_offset_end + 1
    //                     )
    //                 });
    //
    //                 visual_offset_start = visual_offset_end;
    //                 visual_line_index += 1;
    //             }
    //         }
    //     }
    //
    //     self.origin_lines = all_origin_lines;
    //     self.origin_folded_lines = origin_folded_lines;
    //     self.on_update_lines();
    //     Ok(())
    // }

    fn init_origin_line(&self, current_line: usize) -> Result<OriginLine> {
        let start_offset = self.buffer().offset_of_line(current_line)?;
        let end_offset = self.buffer().offset_of_line(current_line + 1)?;
        // let mut fg_styles = Vec::new();
        // 用于存储该行的最高诊断级别。最后决定该行的背景色
        // let mut max_severity: Option<DiagnosticSeverity> = None;
        // fg_styles.extend(self.get_line_diagnostic_styles(
        //     start_offset,
        //     end_offset,
        //     &mut max_severity,
        //     0,
        // ));

        let phantom_text = self.phantom_text(current_line)?;
        let semantic_styles =
            self.get_line_semantic_styles(current_line, start_offset, end_offset);
        let diagnostic_styles = self.get_line_diagnostic_styles_2(
            current_line,
            start_offset,
            end_offset,
        );
        Ok(OriginLine {
            line_index: current_line,
            start_offset,
            len: end_offset - start_offset,
            phantom: phantom_text,
            semantic_styles,
            diagnostic_styles,
        })
    }

    fn get_line_semantic_styles(
        &self,
        origin_line: usize,
        line_start: usize,
        line_end: usize,
    ) -> Vec<NewLineStyle> {
        self._get_line_semantic_styles(origin_line, line_start, line_end)
            .unwrap_or_default()
    }

    fn _get_line_semantic_styles(
        &self,
        origin_line: usize,
        line_start: usize,
        line_end: usize,
    ) -> Option<Vec<NewLineStyle>> {
        Some(
            if self.style_from_lsp {
                &self.semantic_styles.as_ref()?.1
            } else {
                self.syntax.styles.as_ref()?
            }
                .iter()
                .filter_map(|(Interval { start, end }, fg_color)| {
                    if line_start <= start && end < line_end {
                        let color = self.config.syntax_style_color(fg_color)?;
                        Some(NewLineStyle {
                            origin_line,
                            origin_line_offset_start: start - line_start,
                            len: end - start,
                            start_of_buffer: start,
                            end_of_buffer: end,
                            fg_color: color,
                            folded_line_offset_start: start - line_start,
                            folded_line_offset_end: end - line_start,
                        })
                    } else {
                        None
                    }
                })
                .collect()
        )
    }

    // pub fn wrap(&self, viewport: Rect, es: &EditorStyle) ->
    // ResolvedWrap {     match es.wrap_method() {
    //         WrapMethod::None => ResolvedWrap::None,
    //         WrapMethod::EditorWidth => {
    //             ResolvedWrap::Width((viewport.width() as
    // f32).max(MIN_WRAPPED_WIDTH))         }
    //         WrapMethod::WrapColumn { .. } => todo!(),
    //         WrapMethod::WrapWidth { width } =>
    // ResolvedWrap::Width(width),     }
    // }

    /// Set the wrapping style
    ///
    /// Does nothing if the wrapping style is the same as the current
    /// one. Will trigger a clear of the text layouts if the
    /// wrapping style is different.
    // pub fn set_wrap(&mut self, wrap: ResolvedWrap, _editor:
    // &Editor) {     if wrap == self.wrap {
    //         return;
    //     }
    //     self.wrap = wrap;
    //     // self.update(editor);
    // }

    pub fn max_width(&self) -> f64 {
        self.max_width
    }

    /// ~~视觉~~行的text_layout信息
    fn _text_layout_of_visual_line(&self, line: usize) -> Option<&TextLayoutLine> {
        Some(&self.origin_folded_lines.get(self.visual_lines.get(line)?.origin_folded_line)?
            .text_layout)
    }
    pub fn text_layout_of_visual_line(&self, line: usize) -> Result<&TextLayoutLine> {
        self._text_layout_of_visual_line(line).ok_or(anyhow!("text layout empty)"))
    }

    // 原始行的第一个视觉行。原始行可能会有多个视觉行
    pub fn start_visual_line_of_origin_line(
        &self,
        origin_line: usize,
    ) -> Result<&VisualLine> {
        let folded_line = self.folded_line_of_origin_line(origin_line)?;
        self.start_visual_line_of_folded_line(folded_line.line_index)
    }

    pub fn start_visual_line_of_folded_line(
        &self,
        origin_folded_line: usize,
    ) -> Result<&VisualLine> {
        for visual_line in &self.visual_lines {
            if visual_line.origin_folded_line == origin_folded_line {
                return Ok(visual_line);
            }
        }
        bail!(
            "start_visual_line_of_folded_line \
             origin_folded_line={origin_folded_line}"
        )
    }

    pub fn folded_line_of_origin_line(
        &self,
        origin_line: usize,
    ) -> Result<&OriginFoldedLine> {
        for folded_line in &self.origin_folded_lines {
            if folded_line.origin_line_start <= origin_line
                && origin_line <= folded_line.origin_line_end
            {
                return Ok(folded_line);
            }
        }
        bail!("folded_line_of_origin_line origin_line={origin_line}")
    }

    pub fn folded_line_of_visual_line(
        &self,
        vl: &VisualLine,
    ) -> Result<&OriginFoldedLine> {
        for folded_line in &self.origin_folded_lines {
            if folded_line.line_index == vl.origin_folded_line {
                return Ok(folded_line);
            }
        }
        bail!("folded_line_of_visual_line {vl:?}")
    }

    pub fn visual_line_of_folded_line_and_sub_index(
        &self,
        origin_folded_line: usize,
        sub_index: usize,
    ) -> Result<&VisualLine> {
        for visual_line in &self.visual_lines {
            if visual_line.origin_folded_line == origin_folded_line
                && visual_line.origin_folded_line_sub_index == sub_index
            {
                return Ok(visual_line);
            }
        }
        bail!(
            "visual_line_of_folded_line_and_sub_index \
             origin_folded_line={origin_folded_line} sub_index={sub_index}"
        )
    }

    pub fn last_visual_line(&self) -> &VisualLine {
        &self.visual_lines[self.visual_lines.len() - 1]
    }

    pub fn buffer_offset_of_click(
        &self,
        _mode: &CursorMode,
        point: Point,
    ) -> Result<(usize, bool)> {
        let info = self.signals.screen_lines.val().visual_line_of_y(point.y);
        // info.visual_line.origin_line
        let text_layout =
            self.text_layout_of_visual_line(info.visual_line.line_index)?;
        let y = text_layout
            .get_layout_y(info.visual_line.origin_folded_line_sub_index)
            .unwrap_or(0.0);
        let hit_point = text_layout.text.hit_point(Point::new(point.x, y as f64));
        // let index = if hit_point.index {
        //     hit_point.index
        // } else {
        //     hit_point.index.max(1) - 1
        // };
        let (origin_line, origin_col, _offset_of_line) = text_layout
            .phantom_text
            .cursor_position_of_final_col(hit_point.index);
        let offset_of_buffer =
            self.buffer().offset_of_line_col(origin_line, origin_col)?;
        Ok((offset_of_buffer, hit_point.is_inside))
    }

    pub fn result_of_left_click(&mut self, point: Point) -> Result<ClickResult> {
        let info = self.screen_lines().visual_line_of_y(point.y);

        let text_layout =
            self.text_layout_of_visual_line(info.visual_line.line_index)?;
        let y = text_layout
            .get_layout_y(info.visual_line.origin_folded_line_sub_index)
            .unwrap_or(0.0);
        let hit_point = text_layout.text.hit_point(Point::new(point.x, y as f64));
        Ok(
            if let Text::Phantom { text: phantom } =
                text_layout.phantom_text.text_of_final_col(hit_point.index)
            {
                let phantom_offset = hit_point.index - phantom.final_col;
                if let PhantomTextKind::InlayHint = phantom.kind {
                    let line = phantom.line as u32;
                    let index = phantom.col as u32;
                    if let Some(hints) = &self.inlay_hints {
                        if let Some(location) = hints.iter().find_map(|(_, hint)| {
                            if hint.position.line == line
                                && hint.position.character == index
                            {
                                if let InlayHintLabel::LabelParts(parts) =
                                    &hint.label
                                {
                                    let mut start = 0;
                                    for part in parts {
                                        let end = start + part.value.len();
                                        if start <= phantom_offset
                                            && phantom_offset < end
                                        {
                                            return part.location.clone();
                                        }
                                        start = end;
                                    }
                                }
                            }
                            None
                        }) {
                            return Ok(ClickResult::MatchHint(location));
                        }
                    }
                } else if let PhantomTextKind::LineFoldedRang {
                    start_position,
                    ..
                } = phantom.kind
                {
                    self.update_folding_ranges(start_position.into())?;
                    return Ok(ClickResult::MatchFolded);
                }
                ClickResult::MatchWithoutLocation
            } else {
                ClickResult::NoHint
            }
        )
    }

    /// 原始位移字符所在的行信息（折叠行、原始行、视觉行）
    pub fn lines_of_origin_offset(
        &self,
        origin_offset: usize,
    ) -> Result<LinesOfOriginOffset> {
        // 位于的原始行，以及在原始行的起始offset
        let origin_line = self.buffer().line_of_offset(origin_offset);
        let origin_line = self.origin_lines.get(origin_line).ok_or(anyhow!("origin_line is empty"))?.clone();
        let offset = origin_offset - origin_line.start_offset;
        let folded_line = self.folded_line_of_origin_line(origin_line.line_index)?;
        let origin_folded_line_offset = folded_line
            .text_layout
            .phantom_text
            .final_col_of_col(origin_line.line_index, offset, false);
        let folded_line_layout = folded_line.text_layout.text.line_layout();
        let mut sub_line_index = folded_line_layout.len() - 1;
        let mut visual_line_offset = origin_folded_line_offset;
        for (index, sub_line) in folded_line_layout.iter().enumerate() {
            if visual_line_offset < sub_line.glyphs.len() {
                sub_line_index = index;
                break;
            } else {
                visual_line_offset -= sub_line.glyphs.len();
            }
        }
        let visual_line = self.visual_line_of_folded_line_and_sub_index(
            folded_line.line_index,
            sub_line_index,
        )?;
        Ok(LinesOfOriginOffset {
            origin_offset: 0,
            origin_line,
            origin_folded_line: folded_line.clone(),
            origin_folded_line_offest: 0,
            visual_line: visual_line.clone(),
            visual_line_offest: 0,
        })
    }

    /// 视觉行的偏移位置，对应的上一行的偏移位置（原始文本）和是否为最后一个字符
    pub fn previous_visual_line(
        &self,
        visual_line_index: usize,
        line_offset: usize,
        _affinity: CursorAffinity,
    ) -> Result<(VisualLine, usize, bool)> {
        self._previous_visual_line(visual_line_index, line_offset, _affinity).ok_or(anyhow!("visual_line_index={} line_offset={} is empty", visual_line_index, line_offset))
    }
    fn _previous_visual_line(
        &self,
        visual_line_index: usize,
        mut line_offset: usize,
        _affinity: CursorAffinity,
    ) -> Option<(VisualLine, usize, bool)> {
        let prev_visual_line = self.visual_lines.get(visual_line_index.max(1) - 1)?;
        let mut last_char = 0;
        for (index, layout) in self.origin_folded_lines
            .get(prev_visual_line.origin_folded_line)?
            .text_layout
            .text
            .line_layout()
            .iter()
            .enumerate()
        {
            if index < prev_visual_line.origin_folded_line_sub_index {
                line_offset += layout.glyphs.len();
            } else if index >= prev_visual_line.origin_folded_line_sub_index {
                last_char = layout.glyphs.len() - 1;
                break;
            }
        }
        let (_origin_line, offset_line, _offset_buffer) = self.origin_folded_lines
            .get(prev_visual_line.origin_folded_line)?
            .text_layout
            .phantom_text
            .cursor_position_of_final_col(line_offset);
        Some((
            prev_visual_line.clone(),
            offset_line,
            offset_line == last_char
        ))
    }

    /// 视觉行的偏移位置，对应的上一行的偏移位置（原始文本）和是否为最后一个字符
    pub fn next_visual_line(
        &self,
        visual_line_index: usize,
        mut line_offset: usize,
        _affinity: CursorAffinity,
    ) -> (VisualLine, usize, bool) {
        let next_visual_line = &self.visual_lines
            [visual_line_index.min(self.visual_lines.len() - 2) + 1];
        let mut last_char = 0;
        for (index, layout) in self.origin_folded_lines
            [next_visual_line.origin_folded_line]
            .text_layout
            .text
            .line_layout()
            .iter()
            .enumerate()
        {
            if index < next_visual_line.origin_folded_line_sub_index {
                line_offset += layout.glyphs.len();
            } else if index >= next_visual_line.origin_folded_line_sub_index {
                last_char = layout.glyphs.len().max(1) - 1;
                break;
            }
        }
        let (_origin_line, offset_line, _offset_buffer) = self.origin_folded_lines
            [next_visual_line.origin_folded_line]
            .text_layout
            .phantom_text
            .cursor_position_of_final_col(line_offset);
        (
            next_visual_line.clone(),
            offset_line,
            offset_line == last_char
        )
    }

    /// 原始位移字符所在的视觉行，以及视觉行的偏移位置，
    /// 合并行的偏移位置和是否是最后一个字符，point
    pub fn visual_line_of_offset(
        &self,
        offset: usize,
        _affinity: CursorAffinity,
    ) -> Result<(VisualLine, usize, usize, bool, &OriginFoldedLine)> {
        // 位于的原始行，以及在原始行的起始offset
        let (origin_line, offset_of_origin_line) = {
            let origin_line = self.buffer().line_of_offset(offset);
            let origin_line_start_offset =
                self.buffer().offset_of_line(origin_line)?;
            (origin_line, origin_line_start_offset)
        };
        let offset = offset - offset_of_origin_line;
        let folded_line = self.folded_line_of_origin_line(origin_line)?;

        let (sub_line_index, offset_of_visual, offset_of_folded) =
            folded_line.visual_line_of_line_and_offset(origin_line, offset);
        let visual_line = self.visual_line_of_folded_line_and_sub_index(
            folded_line.line_index,
            sub_line_index,
        )?;
        let last_char = offset_of_folded
            >= folded_line.len_without_rn(self.buffer().line_ending());

        Ok((
            visual_line.clone(),
            offset_of_visual,
            offset_of_folded,
            last_char,
            folded_line
        ))
    }

    pub fn visual_lines(&self, start: usize, end: usize) -> Vec<VisualLine> {
        let start = start.min(self.visual_lines.len() - 1);
        let end = end.min(self.visual_lines.len() - 1);

        let mut vline_infos = Vec::with_capacity(end - start + 1);
        for index in start..=end {
            vline_infos.push(self.visual_lines[index].clone());
        }
        vline_infos
    }

    fn phantom_text(&self, line: usize) -> Result<PhantomTextLine> {
        let buffer = self.buffer();
        let (start_offset, end_offset) = (
            buffer.offset_of_line(line)?,
            buffer.offset_of_line(line + 1)?
        );

        let origin_text_len = end_offset - start_offset;
        // lsp返回的字符包括换行符，现在长度不考虑，后续会有问题
        // let line_ending =
        // self.buffer.line_ending().get_chars().len();
        // if origin_text_len >= line_ending {
        //     origin_text_len -= line_ending;
        // }
        // if line == 10 {
        //     info!("start_offset={start_offset}
        // end_offset={end_offset}
        // origin_text_len={origin_text_len}"); }

        let folded_ranges =
            self.folding_ranges.get_folded_range_by_line(line as u32);

        // If hints are enabled, and the hints field is filled, then
        // get the hints for this line and convert them into
        // PhantomText instances
        let hints = self
            .config
            .enable_inlay_hints
            .then_some(())
            .and(self.inlay_hints.as_ref())
            .map(|hints| hints.iter_chunks(start_offset..end_offset))
            .into_iter()
            .flatten()
            .filter(|(interval, hint)| {
                interval.start >= start_offset
                    && interval.start < end_offset
                    && !folded_ranges.contain_position(hint.position)
            })
            .filter_map(|(interval, inlay_hint)| {
                let (col, affinity) = {
                    let mut cursor =
                        lapce_xi_rope::Cursor::new(buffer.text(), interval.start);

                    let next_char = cursor.peek_next_codepoint();
                    let prev_char = cursor.prev_codepoint();

                    let mut affinity = None;
                    if let Some(prev_char) = prev_char {
                        let c = get_char_property(prev_char);
                        if c == CharClassification::Other {
                            affinity = Some(CursorAffinity::Backward)
                        } else if matches!(
                            c,
                            CharClassification::Lf
                                | CharClassification::Cr
                                | CharClassification::Space
                        ) {
                            affinity = Some(CursorAffinity::Forward)
                        }
                    };
                    if affinity.is_none() {
                        if let Some(next_char) = next_char {
                            let c = get_char_property(next_char);
                            if c == CharClassification::Other {
                                affinity = Some(CursorAffinity::Forward)
                            } else if matches!(
                                c,
                                CharClassification::Lf
                                    | CharClassification::Cr
                                    | CharClassification::Space
                            ) {
                                affinity = Some(CursorAffinity::Backward)
                            }
                        }
                    }

                    let (_, col) = match buffer.offset_to_line_col(interval.start) {
                        Ok(rs) => rs,
                        Err(err) => {
                            error!("{err:?}");
                            return None;
                        }
                    };
                    (col, affinity)
                };
                let mut text = match &inlay_hint.label {
                    InlayHintLabel::String(label) => label.to_string(),
                    InlayHintLabel::LabelParts(parts) => {
                        parts.iter().map(|p| &p.value).join("")
                    }
                };
                match (text.starts_with(':'), text.ends_with(':')) {
                    (true, true) => {
                        text.push(' ');
                    }
                    (true, false) => {
                        text.push(' ');
                    }
                    (false, true) => {
                        text = format!(" {} ", text);
                    }
                    (false, false) => {
                        text = format!(" {}", text);
                    }
                }
                Some(PhantomText {
                    kind: PhantomTextKind::InlayHint,
                    col,
                    text,
                    affinity,
                    fg: Some(self.config.inlay_hint_fg),
                    // font_family:
                    // Some(self.config.inlay_hint_font_family()),
                    font_size: Some(self.config.inlay_hint_font_size()),
                    bg: Some(self.config.inlay_hint_bg),
                    under_line: None,
                    final_col: col,
                    line,
                    merge_col: col,
                })
            });
        // You're quite unlikely to have more than six hints on a
        // single line this later has the diagnostics added
        // onto it, but that's still likely to be below six
        // overall.
        let mut text: SmallVec<[PhantomText; 6]> = hints.collect();

        // If error lens is enabled, and the diagnostics field is
        // filled, then get the diagnostics that end on this
        // line which have a severity worse than HINT and convert them
        // into PhantomText instances

        // 会与折叠冲突，因此暂时去掉
        // let mut diag_text: SmallVec<[PhantomText; 6]> = self.config
        //     .enable_error_lens
        //     .then_some(())
        //     .map(|_|
        // self.diagnostics.diagnostics_span.get_untracked())
        //     .map(|diags| {
        //         diags
        //             .iter_chunks(start_offset..end_offset)
        //             .filter_map(|(iv, diag)| {
        //                 let end = iv.end();
        //                 let end_line =
        // self.buffer.line_of_offset(end);                 if
        // end_line == line                     &&
        // diag.severity < Some(DiagnosticSeverity::HINT)
        //                     &&
        // !folded_ranges.contain_position(diag.range.start)
        //                     &&
        // !folded_ranges.contain_position(diag.range.end)
        //                 {
        //                     let fg = {
        //                         let severity = diag
        //                             .severity
        //
        // .unwrap_or(DiagnosticSeverity::WARNING);
        //
        // self.config.color_of_error_lens(severity)
        //                     };
        //
        //                     let text = if
        // self.config.only_render_error_styling {
        // "".to_string()                     } else if
        // self.config.error_lens_multiline {
        // format!("    {}", diag.message)
        // } else {                         format!("    {}",
        // diag.message.lines().join(" "))
        // };                     Some(PhantomText {
        //                         kind: PhantomTextKind::Diagnostic,
        //                         col: end_offset - start_offset,
        //                         affinity:
        // Some(CursorAffinity::Backward),
        // text,                         fg: Some(fg),
        //                         font_size: Some(
        //
        // self.config.error_lens_font_size(),
        // ),                         bg: None,
        //                         under_line: None,
        //                         final_col: end_offset -
        // start_offset,                         line,
        //                         merge_col: end_offset -
        // start_offset,                     })
        //                 } else {
        //                     None
        //                 }
        //             })
        //             .collect::<SmallVec<[PhantomText; 6]>>()
        //     })
        //     .unwrap_or_default();
        //
        // text.append(&mut diag_text);

        let (completion_line, completion_col) = self.completion_pos;
        let completion_text = self.config
            .enable_completion_lens
            .then_some(())
            .and(self.completion_lens.as_ref())
            // TODO: We're probably missing on various useful completion things to include here!
            .filter(|_| {
                line == completion_line
                    && !folded_ranges.contain_position(Position {
                    line: completion_line as u32,
                    character: completion_col as u32,
                })
            })
            .map(|completion| PhantomText {
                kind: PhantomTextKind::Completion,
                col: completion_col,
                text: completion.clone(),
                fg: Some(self.config.completion_lens_foreground),
                font_size: Some(self.config.completion_lens_font_size()),
                affinity: Some(CursorAffinity::Backward),
                // font_family: Some(self.config.editor.completion_lens_font_family()),
                bg: None,
                under_line: None,
                final_col: completion_col,
                line,
                merge_col: completion_col,
                // TODO: italics?
            });
        if let Some(completion_text) = completion_text {
            text.push(completion_text);
        }

        // TODO: don't display completion lens and inline completion
        // at the same time and/or merge them so that they can
        // be shifted between like multiple inline completions
        // can
        // let (inline_completion_line, inline_completion_col) =
        //     self.inline_completion_pos;
        let inline_completion_text = self
            .config
            .enable_inline_completion
            .then_some(())
            .and(self.inline_completion.as_ref())
            .filter(|(_, inline_completion_line, inline_completion_col)| {
                line == *inline_completion_line
                    && !folded_ranges.contain_position(Position {
                    line: *inline_completion_line as u32,
                    character: *inline_completion_col as u32,
                })
            })
            .map(|(completion, _, inline_completion_col)| {
                PhantomText {
                    kind: PhantomTextKind::Completion,
                    col: *inline_completion_col,
                    text: completion.clone(),
                    affinity: Some(CursorAffinity::Backward),
                    fg: Some(self.config.completion_lens_foreground),
                    font_size: Some(self.config.completion_lens_font_size()),
                    // font_family:
                    // Some(self.config.
                    // completion_lens_font_family()),
                    bg: None,
                    under_line: None,
                    final_col: *inline_completion_col,
                    line,
                    merge_col: *inline_completion_col, // TODO: italics?
                }
            });
        if let Some(inline_completion_text) = inline_completion_text {
            text.push(inline_completion_text);
        }

        // todo filter by folded?
        if let Some(preedit) = util::preedit_phantom(
            &self.preedit,
            buffer,
            Some(self.config.editor_foreground),
            line,
        ) {
            text.push(preedit)
        }

        let fg = self.config.inlay_hint_fg;
        let font_size = self.config.inlay_hint_font_size();
        let bg = self.config.inlay_hint_bg;
        text.extend(
            folded_ranges.into_phantom_text(buffer, line, font_size, fg, bg)
        );

        Ok(PhantomTextLine::new(
            line,
            origin_text_len,
            start_offset,
            text,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn new_text_layout_2(
        &self,
        line: usize,
        origins: &[OriginLine],
        attrs: Attrs,
    ) -> Result<(TextLayoutLine, Vec<NewLineStyle>, Vec<NewLineStyle>)> {
        let origin_line =
            origins.get(line).ok_or(anyhow!("origins {line} empty"))?;

        let mut line_content = String::new();

        {
            let line_content_original = self.buffer().line_content(line)?;
            util::push_strip_suffix(&line_content_original, &mut line_content);
        }

        let mut collapsed_line_col = origin_line.phantom.folded_line();
        let mut phantom_text =
            PhantomTextMultiLine::new(origin_line.phantom.clone());

        let mut attrs_list = AttrsList::new(attrs);
        let mut font_system = FONT_SYSTEM.lock();
        let mut semantic_styles = origin_line.semantic_styles(0);
        let mut diagnostic_styles = origin_line.diagnostic_styles(0);

        while let Some(collapsed_line) = collapsed_line_col.take() {
            {
                util::push_strip_suffix(
                    self.buffer().line_content(collapsed_line)?.as_ref(),
                    &mut line_content,
                );
            }
            let offset_col = phantom_text.origin_text_len;
            let next_origin_line = origins
                .get(collapsed_line)
                .ok_or(anyhow!("origins {line} empty"))?;
            let next_phantom_text = next_origin_line.phantom.clone();
            collapsed_line_col = next_phantom_text.folded_line();
            semantic_styles.extend(next_origin_line.semantic_styles(offset_col));
            diagnostic_styles.extend(next_origin_line.diagnostic_styles(offset_col));
            phantom_text.merge(next_phantom_text);
        }

        let phantom_color = self.editor_style.phantom_color();
        phantom_text.add_phantom_style(
            &mut attrs_list,
            attrs.font_size(attrs.font_size - 1.0),
            phantom_color,
        );
        let final_line_content = phantom_text.final_line_content(&line_content);
        self.apply_semantic_styles_2(
            &phantom_text,
            &semantic_styles,
            &mut attrs_list,
            attrs,
        );
        let mut text_layout = TextLayout::new_with_font_system(
            line,
            &final_line_content,
            attrs_list,
            &mut font_system,
        );
        drop(font_system);
        match self.editor_style.wrap_method() {
            WrapMethod::None => {}
            WrapMethod::EditorWidth => {
                text_layout.set_wrap(Wrap::WordOrGlyph);
                text_layout.set_size(self.viewport_size.width as f32, f32::MAX);
            }
            WrapMethod::WrapWidth { width } => {
                text_layout.set_wrap(Wrap::WordOrGlyph);
                text_layout.set_size(width, f32::MAX);
            }
            // TODO:
            WrapMethod::WrapColumn { .. } => {}
        }
        let indent = 0.0;
        let mut layout_line = TextLayoutLine {
            text: text_layout,
            extra_style: Vec::new(),
            whitespaces: None,
            indent,
            phantom_text,
        };
        // 下划线？背景色？
        util::apply_layout_styles(&mut layout_line);
        self.apply_diagnostic_styles_2(&mut layout_line, &diagnostic_styles);

        Ok((layout_line, semantic_styles, diagnostic_styles))
    }

    // pub fn update_folding_item(&mut self, item: FoldingDisplayItem)
    // {     match item.ty {
    //         FoldingDisplayType::UnfoldStart |
    // FoldingDisplayType::Folded => {
    // self.folding_ranges.0.iter_mut().find_map(|range| {
    //                 if range.start == item.position {
    //                     range.status.click();
    //                     Some(())
    //                 } else {
    //                     None
    //                 }
    //             });
    //         }
    //         FoldingDisplayType::UnfoldEnd => {
    //             self.folding_ranges.0.iter_mut().find_map(|range| {
    //                 if range.end == item.position {
    //                     range.status.click();
    //                     Some(())
    //                 } else {
    //                     None
    //                 }
    //             });
    //         }
    //     }
    //     self.update_lines();
    // }

    fn trigger_signals(&mut self) {
        self.signals.trigger();
    }

    pub fn trigger_signals_force(&mut self) {
        self.signals.trigger_force();
    }

    // pub fn update_folding_ranges(&mut self, new: Vec<FoldingRange>)
    // {     self.folding_ranges.update_ranges(new);
    //     self.update_lines();
    // }

    fn update_completion_lens(&mut self, delta: &RopeDelta) -> Result<()> {
        let Some(completion) = &mut self.completion_lens else {
            return Ok(());
        };
        let (line, col) = self.completion_pos;
        let offset = self.signals.buffer.val().offset_of_line_col(line, col)?;
        if delta.as_simple_insert().is_some() {
            let (iv, new_len) = delta.summary();
            if iv.start() == iv.end()
                && iv.start() == offset
                && new_len <= completion.len()
            {
                // Remove the # of newly inserted characters
                // These aren't necessarily the same as the characters
                // literally in the text, but the
                // completion will be updated when the completion
                // widget receives the update event,
                // and it will fix this if needed.
                // TODO: this could be smarter and use the insert's
                // content
                self.completion_lens = Some(completion[new_len..].to_string());
            }
        }

        // Shift the position by the rope delta
        let mut transformer = Transformer::new(delta);

        let new_offset = transformer.transform(offset, true);
        let new_pos = self.buffer().offset_to_line_col(new_offset)?;
        self.completion_pos = new_pos;
        Ok(())
    }

    /// init by lsp
    fn init_diagnostics_with_buffer(&self) -> Result<()> {
        let len = self.buffer().len();
        let diagnostics = self.diagnostics.diagnostics.get_untracked();
        let mut span = SpansBuilder::new(len);
        for diag in diagnostics.into_iter() {
            let start = self.buffer().offset_of_position(&diag.range.start)?;
            let end = self.buffer().offset_of_position(&diag.range.end)?;
            // warn!("start={start} end={end} {:?}", diag);
            span.add_span(Interval::new(start, end), diag);
        }
        let span = span.build();
        self.diagnostics.diagnostics_span.set(span);
        Ok(())
    }

    fn update_diagnostics(&mut self, delta: &RopeDelta) {
        if self
            .diagnostics
            .diagnostics
            .with_untracked(|d| d.is_empty())
        {
            return;
        }

        self.diagnostics.diagnostics_span.update(|diagnostics| {
            diagnostics.apply_shape(delta);
        });
    }

    // /// 语义的样式和方括号的样式
    // fn line_semantic_styles(
    //     &self,
    //     line: usize,
    // ) -> Option<Vec<(usize, usize, Color)>> {
    //     let mut styles: Vec<(usize, usize, Color)> =
    //         self.line_style(line)?;
    //     if let Some(bracket_styles) =
    // self.parser.bracket_pos.get(&line) {         let mut
    // bracket_styles = bracket_styles             .iter()
    //             .filter_map(|bracket_style| {
    //                 if let Some(fg_color) =
    // bracket_style.fg_color.as_ref() {                     if
    // let Some(fg_color) = self.config.syntax_style_color(fg_color) {
    //                         return Some((
    //                             bracket_style.start,
    //                             bracket_style.end,
    //                             fg_color,
    //                         ));
    //                     }
    //                 }
    //                 None
    //             })
    //             .collect();
    //         styles.append(&mut bracket_styles);
    //     }
    //     Some(styles)
    // }

    // // 文本样式，前景色
    // fn line_style(
    //     &self,
    //     line: usize,
    // ) -> Option<Vec<(usize, usize, Color)>> {
    //     // let styles = self.styles();
    //     let styles = self.line_styles.get(&line)?;
    //     Some(
    //         styles
    //             .iter()
    //             .filter_map(|x| {
    //                 if let Some(fg) = &x.fg_color {
    //                     if let Some(color) =
    // self.config.syntax_style_color(fg) {
    // return Some((
    // x.origin_line_offset_start,
    // x.origin_line_offset_end,
    // color,                         ));
    //                     }
    //                 }
    //                 None
    //             })
    //             .collect(),
    //     )
    // }

    // fn indent_line(
    //     &self,
    //     line: usize,
    //     line_content: &str,
    // ) -> usize {
    //     if line_content.trim().is_empty() {
    //         let offset = self.buffer.offset_of_line(line);
    //         if let Some(offset) = self.syntax.parent_offset(offset)
    // {             return self.buffer.line_of_offset(offset);
    //         }
    //     }
    //     line
    // }

    fn _compute_screen_lines(&mut self, base: Rect) -> ScreenLines {
        debug!("_compute_screen_lines");
        // TODO: this should probably be a get since we need to depend
        // on line-height let doc_lines =
        // doc.doc_lines.get_untracked();
        let view_kind = self.kind.get_untracked();
        // let base = self.screen_lines().base;

        let line_height = self.line_height;
        let (y0, y1) = (base.y0, base.y1);
        // Get the start and end (visual) lines that are visible in
        // the viewport
        let min_val = (y0 / line_height as f64).floor() as usize;
        let max_val = (y1 / line_height as f64).floor() as usize;
        let vline_infos = self.visual_lines(min_val, max_val);
        util::compute_screen_lines(view_kind, base, vline_infos, line_height, y0)
    }

    pub fn viewport(&self) -> Rect {
        self.screen_lines().base
    }

    pub fn log(&self) {
        info!(
            "DocLines viewport={:?} buffer.rev={} buffer.len()=[{}] \
             style_from_lsp={} is_pristine={} base={:?}",
            self.viewport_size,
            self.buffer().rev(),
            self.buffer().text().len(),
            self.style_from_lsp,
            self.buffer().is_pristine(),
            self.screen_lines().base
        );
        // info!("{:?}", self.config);
        // for origin_lines in &self.origin_lines {
        //     info!("{:?}", origin_lines);
        // }
        // self._log_folded_lines();
        // self._log_visual_lines();
        // self._log_screen_lines();
        // info!("folding_items");
        for item in self.signals.folding_items.val() {
            info!("{:?}", item);
        }
        self._log_folding_ranges();
    }

    pub fn _log_folding_ranges(&self) {
        info!("folding_ranges");
        for range in &self.folding_ranges.0 {
            info!("{:?}", range);
        }
    }
    pub fn _log_folded_lines(&self) {
        for origin_folded_line in &self.origin_folded_lines {
            info!("{:?}", origin_folded_line);
        }
    }
    pub fn _log_screen_lines(&self) {
        info!("screen_lines");
        info!(
            "base={:?}",
            self.screen_lines().base
        );
        for visual_line in &self.screen_lines().visual_lines {
            info!("{:?}", visual_line);
        }
    }

    pub fn _log_visual_lines(&self) {
        for visual_line in &self.visual_lines {
            info!("{:?}", visual_line);
        }
    }

    fn apply_semantic_styles_2(
        &self,
        phantom_text: &PhantomTextMultiLine,
        semantic_styles: &[NewLineStyle],
        attrs_list: &mut AttrsList,
        attrs: Attrs,
    ) {
        for NewLineStyle {
            origin_line_offset_start,
            len,
            fg_color,
            ..
        } in semantic_styles.iter()
        {
            // for (start, end, color) in styles.into_iter() {
            let (Some(start), Some(end)) = (
                phantom_text.col_at(*origin_line_offset_start),
                phantom_text.col_at(*origin_line_offset_start + *len)
            ) else {
                continue;
            };
            attrs_list.add_span(start..end, attrs.color(*fg_color));
        }
    }

    fn apply_diagnostic_styles_2(
        &self,
        layout_line: &mut TextLayoutLine,
        line_styles: &Vec<NewLineStyle>,
    ) {
        let layout = &layout_line.text;
        let phantom_text = &layout_line.phantom_text;
        // 暂不考虑
        for NewLineStyle {
            origin_line_offset_start: start,
            len,
            fg_color,
            ..
        } in line_styles
        {
            // warn!("line={} start={start}, end={end},
            // color={color:?}", phantom_text.line);
            // col_at(end)可以为空，因为end是不包含的
            let (Some(start), Some(end)) = (
                phantom_text.col_at(*start),
                phantom_text.col_at((*start + *len).max(1) - 1)
            ) else {
                warn!(
                    "line={} start={start}, len={len}, color={fg_color:?} col_at \
                     empty",
                    phantom_text.line
                );
                continue;
            };
            let styles = util::extra_styles_for_range(
                layout,
                start,
                end + 1,
                None,
                None,
                Some(*fg_color),
            );
            layout_line.extra_style.extend(styles);
        }
    }

    // fn apply_diagnostic_styles(
    //     &self,
    //     layout_line: &mut TextLayoutLine,
    //     line_styles: Vec<(usize, usize, Color)>,
    //     // _max_severity: Option<DiagnosticSeverity>,
    // ) {
    //     let layout = &layout_line.text;
    //     let phantom_text = &layout_line.phantom_text;
    //
    //     // 暂不考虑
    //     for (start, end, color) in line_styles {
    //         // warn!("line={} start={start}, end={end},
    // color={color:?}", phantom_text.line);         //
    // col_at(end)可以为空，因为end是不包含的         let
    // (Some(start), Some(end)) = (phantom_text.col_at(start),
    // phantom_text.col_at(end.max(1) - 1)) else {
    // warn!("line={} start={start}, end={end}, color={color:?} col_at
    // empty", phantom_text.line);             continue;
    //         };
    //         let styles =
    //             util::extra_styles_for_range(layout, start, end +
    // 1, None, None, Some(color));         layout_line.
    // extra_style.extend(styles);     }
    //
    //     // 不要背景色，因此暂时comment
    //     // Add the styling for the diagnostic severity, if
    // applicable     // if let Some(max_severity) = max_severity
    // {     //     let size = layout_line.text.size();
    //     //     let x1 = if !config.error_lens_end_of_line {
    //     //         let error_end_x = size.width;
    //     //         Some(error_end_x.max(size.width))
    //     //     } else {
    //     //         None
    //     //     };
    //     //
    //     //     // TODO(minor): Should we show the background only
    // on wrapped lines that have the     //     // diagnostic
    // actually on that line?     //     // That would make it
    // more obvious where it is from and matches other editors.
    //     //     layout_line.extra_style.push(LineExtraStyle {
    //     //         x: 0.0,
    //     //         y: 0.0,
    //     //         width: x1,
    //     //         height: size.height,
    //     //         bg_color:
    // Some(self.config.color_of_error_lens(max_severity)),     //
    // under_line: None,     //         wave_line: None,
    //     //     });
    //     // }
    // }

    /// return (line,start, end, color)
    pub fn get_line_diagnostic_styles(
        &self,
        start_offset: usize,
        end_offset: usize,
        max_severity: &mut Option<DiagnosticSeverity>,
        line_offset: usize,
    ) -> Vec<(usize, usize, Color)> {
        self.config
            .enable_error_lens
            .then_some(())
            .map(|_| {
                self.diagnostics.diagnostics_span.with_untracked(|diags| {
                    diags
                        .iter_chunks(start_offset..end_offset)
                        .filter_map(|(iv, diag)| {
                            let start = iv.start();
                            let end = iv.end();
                            let severity = diag.severity?;
                            // warn!("start_offset={start_offset}
                            // end_offset={end_offset}
                            // interval={iv:?}");
                            if start <= end_offset
                                && start_offset <= end
                                && severity < DiagnosticSeverity::HINT
                            {
                                match (severity, *max_severity) {
                                    (severity, Some(max)) => {
                                        if severity < max {
                                            *max_severity = Some(severity);
                                        }
                                    }
                                    (severity, None) => {
                                        *max_severity = Some(severity);
                                    }
                                }
                                let color =
                                    self.config.color_of_diagnostic(severity)?;
                                Some((
                                    start + line_offset - start_offset,
                                    end + line_offset - start_offset,
                                    color
                                ))
                            } else {
                                None
                            }
                        })
                        .collect()
                })
            })
            .unwrap_or_default()
    }

    /// return (line,start, end, color)
    fn get_line_diagnostic_styles_2(
        &self,
        origin_line: usize,
        start_offset: usize,
        end_offset: usize, /* max_severity: &mut
                           * Option<DiagnosticSeverity>, */
    ) -> Vec<NewLineStyle> {
        self.config
            .enable_error_lens
            .then_some(())
            .map(|_| {
                self.diagnostics.diagnostics_span.with_untracked(|diags| {
                    diags
                        .iter_chunks(start_offset..end_offset)
                        .filter_map(|(iv, diag)| {
                            let start = iv.start();
                            let end = iv.end();
                            let severity = diag.severity?;
                            // warn!("start_offset={start_offset}
                            // end_offset={end_offset}
                            // interval={iv:?}");
                            if start <= end_offset
                                && start_offset <= end
                                && severity < DiagnosticSeverity::HINT
                            {
                                // match (severity, *max_severity)
                                // {
                                //     (severity, Some(max)) => {
                                //         if severity < max {
                                //             *max_severity =
                                // Some(severity);
                                //         }
                                //     }
                                //     (severity, None) => {
                                //         *max_severity =
                                // Some(severity);
                                //     }
                                // }
                                let color =
                                    self.config.color_of_diagnostic(severity)?;
                                Some(NewLineStyle {
                                    origin_line,
                                    origin_line_offset_start: start - start_offset,
                                    len: end - start,
                                    start_of_buffer: start_offset,
                                    end_of_buffer: end_offset,
                                    fg_color: color,
                                    folded_line_offset_start: start - start_offset,
                                    folded_line_offset_end: end - start_offset,
                                })
                            } else {
                                None
                            }
                        })
                        .collect()
                })
            })
            .unwrap_or_default()
    }

    fn update_inlay_hints(&mut self, delta: &RopeDelta) {
        if let Some(hints) = self.inlay_hints.as_mut() {
            hints.apply_shape(delta);
        }
    }

    fn update_folding_display_items(&mut self) {
        let display_items =
            self.folding_ranges.to_display_items(self.screen_lines());
        self.signals
            .folding_items
            .update_if_not_equal(display_items);
    }

    pub fn move_up(
        &self,
        offset: usize,
        affinity: CursorAffinity,
        horiz: Option<ColPosition>,
        _mode: Mode,
        _count: usize,
    ) -> Result<(usize, ColPosition, CursorAffinity)> {
        let (visual_line, line_offset, ..) =
            self.visual_line_of_offset(offset, affinity)?;
        let (previous_visual_line, line_offset, ..) =
            self.previous_visual_line(visual_line.line_index, line_offset, affinity)?;
        let horiz = horiz.unwrap_or_else(|| {
            ColPosition::Col(
                match self.line_point_of_visual_line_col(
                    visual_line.line_index,
                    line_offset,
                    affinity,
                    false,
                ) {
                    Ok(point) => { point.x }
                    Err(err) => {
                        error!("{:?}", err);
                        0.0
                    }
                }
            )
        });

        let offset_of_buffer = self.rvline_horiz_col(
            &horiz,
            _mode != Mode::Normal,
            &previous_visual_line,
        )?;

        // TODO: this should maybe be doing `new_offset ==
        // info.interval.start`?
        let affinity = if line_offset == 0 {
            CursorAffinity::Forward
        } else {
            CursorAffinity::Backward
        };
        Ok((offset_of_buffer, horiz, affinity))
    }

    pub fn end_of_line(
        &self,
        affinity: &mut CursorAffinity,
        offset: usize,
        _mode: Mode,
    ) -> Result<(usize, ColPosition)> {
        let (vl, _offset_of_visual, _offset_folded, _last_char, _) =
            self.visual_line_of_offset(offset, *affinity)?;
        // let new_col = info.last_col(view.text_prov(), mode !=
        // Mode::Normal); let vline_end =
        // vl.visual_interval.end; let start_offset =
        // vl.visual_interval.start; // If these subtractions
        // crash, then it is likely due to a bad vline being kept
        // around // somewhere
        // let new_col = if mode == Mode::Normal &&
        // !vl.visual_interval.is_empty() {
        //     let vline_pre_end =
        // self.buffer().prev_grapheme_offset(vline_end, 1, 0);
        //     vline_pre_end - start_offset
        // } else {
        //     vline_end - start_offset
        // };

        let origin_folded_line = self
            .origin_folded_lines
            .get(vl.origin_folded_line)
            .ok_or(anyhow!("origin_folded_line is not exist"))?;
        *affinity = if origin_folded_line.origin_interval.is_empty() {
            CursorAffinity::Forward
        } else {
            CursorAffinity::Backward
        };
        let new_offset = self.buffer().offset_of_line_col(
            origin_folded_line.origin_line_end,
            origin_folded_line.origin_interval.end,
        )?;

        Ok((new_offset, ColPosition::End))
    }

    pub fn move_down(
        &self,
        offset: usize,
        affinity: CursorAffinity,
        horiz: Option<ColPosition>,
        _mode: Mode,
        _count: usize,
    ) -> Result<(usize, ColPosition, CursorAffinity)> {
        let (visual_line, line_offset, ..) =
            self.visual_line_of_offset(offset, affinity)?;
        let (next_visual_line, next_line_offset, ..) =
            self.next_visual_line(visual_line.line_index, line_offset, affinity);
        let horiz = horiz.unwrap_or_else(|| {
            ColPosition::Col(
                match self.line_point_of_visual_line_col(
                    visual_line.line_index,
                    line_offset,
                    affinity,
                    false,
                ) {
                    Ok(point) => { point.x }
                    Err(err) => {
                        error!("{:?}", err);
                        0.0
                    }
                }
            )
        });
        let offset_of_buffer =
            self.rvline_horiz_col(&horiz, _mode != Mode::Normal, &next_visual_line)?;
        let affinity = if next_line_offset == 0 {
            CursorAffinity::Forward
        } else {
            CursorAffinity::Backward
        };
        warn!("offset_of_buffer={offset_of_buffer} horiz={horiz:?}");

        Ok((offset_of_buffer, horiz, affinity))
    }

    fn rvline_horiz_col(
        &self,
        horiz: &ColPosition,
        _caret: bool,
        visual_line: &VisualLine,
    ) -> Result<usize> {
        Ok(match *horiz {
            ColPosition::Col(x) => {
                let text_layout =
                    self.text_layout_of_visual_line(visual_line.line_index)?;
                let y_pos = text_layout
                    .text
                    .layout_runs()
                    .nth(visual_line.origin_folded_line_sub_index)
                    .map(|run| run.line_y)
                    .or_else(|| {
                        text_layout.text.layout_runs().last().map(|run| run.line_y)
                    })
                    .unwrap_or(0.0);
                let hit_point =
                    text_layout.text.hit_point(Point::new(x, y_pos as f64));
                let n = hit_point.index;
                let rs = text_layout.phantom_text.cursor_position_of_final_col(n);
                rs.2 + rs.1
            }
            ColPosition::End => visual_line.origin_interval.end,
            ColPosition::Start => visual_line.origin_interval.start,
            ColPosition::FirstNonBlank => {
                let text_layout =
                    self.text_layout_of_visual_line(visual_line.line_index)?;

                let final_offset =
                    text_layout.text.line().text()[visual_line.visual_interval.start
                        ..visual_line.visual_interval.end]
                        .char_indices()
                        .find(|(_, c)| !c.is_whitespace())
                        .map(|(idx, _)| visual_line.visual_interval.start + idx)
                        .unwrap_or(visual_line.visual_interval.end);
                let rs = text_layout
                    .phantom_text
                    .cursor_position_of_final_col(final_offset);
                rs.2 + rs.1
            }
        })
    }

    fn update_screen_lines(&mut self) {
        let screen_lines = self._compute_screen_lines(*self.signals.viewport.val());
        self.signals.screen_lines.update_force(screen_lines);
    }

    fn _compute_change_lines(
        &self,
        deltas: &[(Rope, RopeDelta, InvalLines)],
    ) -> Result<OriginLinesDelta> {
        if deltas.len() == 1 {
            if let Some(delta) = deltas.first() {
                return resolve_delta_rs(&delta.0, &delta.1);
            }
        }
        Ok(OriginLinesDelta::default())
    }


    // /// return [start...end), (start...end]
    // #[allow(clippy::type_complexity)]
    // fn compute_change_lines(
    //     &self,
    //     deltas: &[(Rope, RopeDelta, InvalLines)]
    // ) -> Result<OriginLinesDelta> {
    //     let rs = self._compute_change_lines(deltas);
    //     rs
    // }

    #[inline]
    pub fn buffer(&self) -> &Buffer {
        self.signals.buffer.val()
    }

    #[inline]
    fn buffer_mut(&mut self) -> &mut Buffer {
        self.signals.buffer.val_mut()
    }
}

type ComputeLines = DocLines;

impl ComputeLines {
    pub fn first_non_blank(
        &self,
        affinity: &mut CursorAffinity,
        offset: usize,
    ) -> Result<(usize, ColPosition)> {
        let (info, _offset_of_visual, _offset_folded, _last_char, _) =
            self.visual_line_of_offset(offset, *affinity)?;
        let non_blank_offset =
            WordCursor::new(self.buffer().text(), info.origin_interval.start)
                .next_non_blank_char();

        let start_line_offset = info.origin_interval.start;
        // TODO: is this always the correct affinity? It might be
        // desirable for the very first character on a wrapped line?
        *affinity = CursorAffinity::Forward;

        Ok(if offset > non_blank_offset {
            // Jump to the first non-whitespace character if we're
            // strictly after it
            (non_blank_offset, ColPosition::FirstNonBlank)
        } else {
            // If we're at the start of the line, also jump to the
            // first not blank
            if start_line_offset == offset {
                (non_blank_offset, ColPosition::FirstNonBlank)
            } else {
                // Otherwise, jump to the start of the line
                (start_line_offset, ColPosition::Start)
            }
        })
    }
    pub fn line_point_of_visual_line_col(
        &self,
        visual_line: usize,
        col: usize,
        affinity: CursorAffinity,
        _force_affinity: bool,
    ) -> Result<Point> {
        self._line_point_of_visual_line_col(visual_line, col, affinity, _force_affinity).ok_or(anyhow!("visual_line={visual_line} col={col} is empty"))
    }
    pub fn _line_point_of_visual_line_col(
        &self,
        visual_line: usize,
        col: usize,
        affinity: CursorAffinity,
        _force_affinity: bool,
    ) -> Option<Point> {
        let text_layout = &self.origin_folded_lines
            .get(self.visual_lines.get(visual_line)?.origin_folded_line)?
            .text_layout;
        Some(hit_position_aff(
            &text_layout.text,
            col,
            affinity == CursorAffinity::Backward,
        )
            .point)
    }

    #[allow(clippy::type_complexity)]
    /// return (visual line of offset, offset of visual line, offset
    /// of folded line, is last char, viewport position of cursor,
    /// line_height, origin position of cursor)
    ///
    /// last_char should be check in future
    pub fn cursor_position_of_buffer_offset(
        &self,
        offset: usize,
        affinity: CursorAffinity,
    ) -> Result<(
        VisualLine,
        usize,
        usize,
        bool,
        Option<Point>,
        f64,
        Point,
        usize
    )> {
        let (vl, offset_of_visual, offset_folded, last_char, _) =
            self.visual_line_of_offset(offset, affinity)?;
        let mut viewpport_point = hit_position_aff(
            &self.text_layout_of_visual_line(vl.line_index)?.text,
            offset_folded,
            true,
        )
            .point;
        let line_height = self.screen_lines().line_height;
        let screen_line = self.screen_lines().visual_line_info_of_visual_line(&vl);

        let point = if let Some(vlinfo) = screen_line {
            // ?
            // viewpport_point.y = self.screen_lines().base.y0 +
            // screen_line.vline_y;
            viewpport_point.y = vlinfo.visual_line_y;
            viewpport_point.add_assign(self.screen_lines().base.origin().to_vec2());
            Some(viewpport_point)
        } else {
            None
        };
        let mut origin_point = viewpport_point;
        origin_point.y = vl.line_index as f64 * line_height;

        Ok((
            vl,
            offset_of_visual,
            offset_folded,
            last_char,
            point,
            line_height,
            origin_point,
            self.line_height
        ))
    }

    pub fn char_rect_in_viewport(&self, offset: usize) -> Result<Vec<Rect>> {
        // let Ok((vl, _col, col_2, _, folded_line)) =
        // self.visual_line_of_offset(offset, CursorAffinity::Forward)
        // else {     error!("visual_line_of_offset
        // offset={offset} not exist");     return None
        // };
        // let rs = self.screen_lines().
        // visual_line_info_of_visual_line(&vl)?; let mut hit0
        // = folded_line.text_layout.text.hit_position(col_2);
        // let mut hit1 =
        // folded_line.text_layout.text.hit_position(col_2 + 1);
        // hit0.point.y += rs.y;
        // hit1.point.y += rs.y + self.line_height as f64;
        // Some((hit0.point, hit1.point))
        self.normal_selection(offset, offset + 1)
    }

    pub fn normal_selection(
        &self,
        start_offset: usize,
        end_offset: usize,
    ) -> Result<Vec<Rect>> {
        let (vl_start, _col, col_start, _, folded_line_start) =
            self.visual_line_of_offset(start_offset, CursorAffinity::Forward)?;
        let (vl_end, _col, col_end, _, folded_line_end) =
            self.visual_line_of_offset(end_offset, CursorAffinity::Forward)?;
        let Some(rs_start) = self
            .screen_lines()
            .most_up_visual_line_info_of_visual_line(&vl_start)
        else {
            return Ok(vec![]);
        };
        let Some(rs_end) = self
            .screen_lines()
            .most_down_visual_line_info_of_visual_line(&vl_end)
        else {
            return Ok(vec![]);
        };

        let base = self.screen_lines().base.origin().to_vec2();
        if vl_start == vl_end {
            let rs = folded_line_start.line_scope(
                col_start,
                col_end,
                self.line_height as f64,
                rs_start.folded_line_y,
                base,
            );
            // Rect::from(rs).with_origin()
            Ok(vec![rs])
        } else {
            let mut first =
                Vec::with_capacity(vl_end.line_index - vl_start.line_index + 1);
            first.push(folded_line_start.line_scope(
                col_start,
                vl_start.visual_interval.end,
                self.line_height as f64,
                rs_start.folded_line_y,
                base,
            ));

            for vl in &self.screen_lines().visual_lines {
                if vl.visual_line.line_index >= vl_end.line_index {
                    break;
                } else if vl.visual_line.line_index <= vl_start.line_index {
                    continue;
                } else {
                    let Ok(folded_line) =
                        self.folded_line_of_visual_line(&vl.visual_line)
                    else {
                        error!(
                            "folded_line_of_visual_line {:?} not exist",
                            vl.visual_line
                        );
                        continue;
                    };
                    let selection = folded_line.line_scope(
                        vl.visual_line.visual_interval.start,
                        vl.visual_line.visual_interval.end,
                        self.line_height as f64,
                        vl.folded_line_y,
                        base,
                    );
                    first.push(selection)
                }
            }
            let last = folded_line_end.line_scope(
                0,
                col_end,
                self.line_height as f64,
                rs_end.folded_line_y,
                base,
            );
            first.push(last);
            Ok(first)
        }
    }
}

type LinesOnUpdate = DocLines;

impl LinesOnUpdate {
    fn on_update_buffer(&mut self) -> Result<()> {
        if self.syntax.styles.is_some() {
            self.parser
                .update_code(self.signals.buffer.val(), Some(&self.syntax))?;
        } else {
            self.parser.update_code(self.signals.buffer.val(), None)?;
        }
        self.init_diagnostics_with_buffer()?;
        Ok(())
    }

    fn on_update_lines(&mut self) {
        self.max_width = 0.0;
        self.origin_folded_lines.iter().for_each(|x| if x.text_layout.text.size().width > self.max_width {
            self.max_width = x.text_layout.text.size().width;
        });

        self.signals
            .last_line
            .update_if_not_equal(self.compute_last_width(self.buffer().last_line() + 1));
    }

    fn compute_last_width(&self, last_line: usize) -> (usize, f64) {
        let family =
            Cow::Owned(FamilyOwned::parse_list(&self.config.font_family).collect());
        // 设置字体属性
        let attrs = self.init_attrs_without_color(&family); // 等宽字体
        let attrs_list = AttrsList::new(attrs);
        let mut font_system = FONT_SYSTEM.lock();
        // 创建文本缓冲区
        let text_buffer = TextLayout::new_with_font_system(0, last_line.to_string(), attrs_list, &mut font_system);
        (last_line, text_buffer.size().width)
    }
}

type PubUpdateLines = DocLines;

pub enum EditBuffer<'a> {
    Init(Rope),
    SetLineEnding(LineEnding),
    EditBuffer {
        iter: &'a [(Selection, &'a str)],
        edit_type: EditType,
        response: &'a mut Vec<(Rope, RopeDelta, InvalLines)>,
    },
    SetPristine(u64),
    Reload {
        content: Rope,
        set_pristine: bool,
        response: &'a mut Vec<(Rope, RopeDelta, InvalLines)>,
    },
    ExecuteMotionMode {
        cursor: &'a mut Cursor,
        motion_mode: MotionMode,
        range: Range<usize>,
        is_vertical: bool,
        register: &'a mut Register,
        response: &'a mut Vec<(Rope, RopeDelta, InvalLines)>,
    },
    DoEditBuffer {
        cursor: &'a mut Cursor,
        cmd: &'a EditCommand,
        modal: bool,
        register: &'a mut Register,
        smart_tab: bool,
        response: &'a mut Vec<(Rope, RopeDelta, InvalLines)>,
    },
    DoInsertBuffer {
        cursor: &'a mut Cursor,
        s: &'a str,
        response: &'a mut Vec<(Rope, RopeDelta, InvalLines)>,
    },
    SetCursor {
        before_cursor: CursorMode,
        after_cursor: CursorMode,
    },
}

impl Debug for EditBuffer<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            EditBuffer::Init(val) => {
                write!(f, "EditBuffer::Init {:?}", val)
            }
            EditBuffer::SetLineEnding(val) => {
                write!(f, "EditBuffer::SetLineEnding {:?}", val)
            }
            EditBuffer::EditBuffer { iter, edit_type, .. } => {
                write!(f, "EditBuffer::Init iter {:?} edit_type{edit_type:?}", iter, )
            }
            EditBuffer::SetPristine(val) => {
                write!(f, "EditBuffer::SetPristine {:?}", val)
            }
            EditBuffer::Reload { content, set_pristine, .. } => {
                write!(f, "EditBuffer::Reload set_pristine {set_pristine:?} content={content:?}")
            }
            EditBuffer::ExecuteMotionMode { cursor, motion_mode, range, is_vertical, .. } => {
                write!(f, "EditBuffer::ExecuteMotionMode {:?} {motion_mode:?} range={range:?}, is_vertical={is_vertical}", cursor.mode())
            }
            EditBuffer::DoEditBuffer { cursor, cmd, modal, smart_tab, .. } => {
                write!(f, "EditBuffer::DoEditBuffer {:?} {cmd:?} modal={modal} smart_tab={smart_tab}", cursor.mode())
            }
            EditBuffer::DoInsertBuffer { cursor, s, .. } => {
                write!(f, "EditBuffer::DoInsertBuffer {:?} s={s:?}", cursor.mode())
            }
            EditBuffer::SetCursor { before_cursor, after_cursor } => {
                write!(f, "EditBuffer::SetCursor before_cursor {before_cursor:?} after_cursor={after_cursor:?}")
            }
        }
    }
}

impl PubUpdateLines {
    pub fn init_buffer(&mut self, content: Rope) -> Result<bool> {
        self.buffer_edit(EditBuffer::Init(content))
    }

    pub fn buffer_edit(&mut self, edit: EditBuffer) -> Result<bool> {
        debug!("buffer_edit {edit:?}");
        let mut line_delta = OriginLinesDelta::default();
        match edit {
            EditBuffer::Init(content) => {
                let indent =
                    IndentStyle::from_str(self.syntax.language.indent_unit());
                self.buffer_mut().init_content(content);
                self.buffer_mut().detect_indent(|| indent);
            }
            EditBuffer::SetLineEnding(line_ending) => {
                self.buffer_mut().set_line_ending(line_ending);
            }
            EditBuffer::EditBuffer {
                iter,
                edit_type,
                response
            } => {
                let rs = self.buffer_mut().edit(iter, edit_type);
                debug!("buffer_edit EditBuffer {:?} {:?}", rs.1, rs.2);
                self.apply_delta(&rs.1)?;
                line_delta = resolve_delta_rs(&rs.0, &rs.1)?;
                response.push(rs);
            }
            EditBuffer::SetPristine(recv) => {
                return if recv == self.buffer().rev() {
                    self.buffer_mut().set_pristine();
                    self.signals.pristine.update_if_not_equal(true);
                    self.trigger_signals();
                    Ok(true)
                } else {
                    Ok(false)
                };
            }
            EditBuffer::Reload {
                content,
                set_pristine,
                response
            } => {
                let rs = self.buffer_mut().reload(content, set_pristine);
                debug!("buffer_edit Reload {:?} {:?}", rs.1, rs.2);
                self.apply_delta(&rs.1)?;
                // line_delta = self._compute_change_lines_one(&rs)?;
                response.push(rs);
            }
            EditBuffer::ExecuteMotionMode {
                cursor,
                motion_mode,
                range,
                is_vertical,
                register,
                response
            } => {
                *response = Action::execute_motion_mode(
                    cursor,
                    self.buffer_mut(),
                    motion_mode,
                    range,
                    is_vertical,
                    register,
                );
                for delta in &*response {
                    self.apply_delta(&delta.1)?;
                }
                line_delta = self._compute_change_lines(&*response)?;
            }
            EditBuffer::DoEditBuffer {
                cursor,
                cmd,
                modal,
                register,
                smart_tab,
                response
            } => {
                let syntax = &self.syntax;
                let mut clipboard = SystemClipboard::new();
                let old_cursor = cursor.mode().clone();
                *response = Action::do_edit(
                    cursor,
                    self.signals.buffer.val_mut(),
                    cmd,
                    &mut clipboard,
                    register,
                    EditConf {
                        comment_token: syntax.language.comment_token(),
                        modal,
                        smart_tab,
                        keep_indent: true,
                        auto_indent: true,
                    },
                );
                if !response.is_empty() {
                    self.buffer_mut().set_cursor_before(old_cursor);
                    self.buffer_mut().set_cursor_after(cursor.mode().clone());
                    for delta in &*response {
                        self.apply_delta(&delta.1)?;
                    }
                }
                line_delta = self._compute_change_lines(&*response)?;
            }
            EditBuffer::DoInsertBuffer {
                cursor,
                s,
                response
            } => {
                let auto_closing_matching_pairs =
                    self.config.auto_closing_matching_pairs;
                let auto_surround = self.config.auto_surround;
                let old_cursor = cursor.mode().clone();
                let syntax = &self.syntax;
                *response = Action::insert(
                    cursor,
                    self.signals.buffer.val_mut(),
                    s,
                    &|buffer, c, offset| {
                        util::syntax_prev_unmatched(buffer, syntax, c, offset)
                    },
                    auto_closing_matching_pairs,
                    auto_surround,
                );
                self.buffer_mut().set_cursor_before(old_cursor);
                self.buffer_mut().set_cursor_after(cursor.mode().clone());
                for delta in &*response {
                    self.apply_delta(&delta.1)?;
                }
                line_delta = self._compute_change_lines(&*response)?;
            }
            EditBuffer::SetCursor {
                before_cursor,
                after_cursor
            } => {
                self.buffer_mut().set_cursor_after(after_cursor);
                self.buffer_mut().set_cursor_before(before_cursor);
                return Ok(false);
            }
        }
        self.signals
            .pristine
            .update_if_not_equal(self.buffer().is_pristine());
        self.signals
            .buffer_rev
            .update_if_not_equal(self.buffer().rev());
        self.on_update_buffer()?;
        self.update_lines_new(line_delta)?;
        self.on_update_lines();
        self.update_screen_lines();
        self.update_folding_display_items();

        self.trigger_signals();
        Ok(true)
    }

    pub fn set_line_ending(&mut self, line_ending: LineEnding) -> Result<()> {
        self.buffer_edit(EditBuffer::SetLineEnding(line_ending))?;
        Ok(())
    }

    pub fn edit_buffer(
        &mut self,
        iter: &[(Selection, &str)],
        edit_type: EditType,
    ) -> Result<(Rope, RopeDelta, InvalLines)> {
        let mut rs = Vec::with_capacity(1);
        self.buffer_edit(EditBuffer::EditBuffer {
            edit_type,
            iter,
            response: &mut rs,
        })?;
        Ok(rs.remove(0))
    }

    pub fn reload_buffer(
        &mut self,
        content: Rope,
        set_pristine: bool,
    ) -> Result<(Rope, RopeDelta, InvalLines)> {
        let mut rs = Vec::with_capacity(1);
        self.buffer_edit(EditBuffer::Reload {
            content,
            set_pristine,
            response: &mut rs,
        })?;
        Ok(rs.remove(0))
    }

    pub fn set_pristine(&mut self, rev: u64) -> Result<bool> {
        self.buffer_edit(EditBuffer::SetPristine(rev))
    }

    pub fn set_cursor(
        &mut self,
        before_cursor: CursorMode,
        after_cursor: CursorMode,
    ) {
        if let Err(err) = self.buffer_edit(EditBuffer::SetCursor {
            before_cursor,
            after_cursor,
        }) {
            error!("{err:?}");
        }
    }

    pub fn execute_motion_mode(
        &mut self,
        cursor: &mut Cursor,
        motion_mode: MotionMode,
        range: Range<usize>,
        is_vertical: bool,
        register: &mut Register,
    ) -> Result<Vec<(Rope, RopeDelta, InvalLines)>> {
        let mut rs = Vec::with_capacity(1);
        self.buffer_edit(EditBuffer::ExecuteMotionMode {
            cursor,
            motion_mode,
            range,
            is_vertical,
            register,
            response: &mut rs,
        })?;
        Ok(rs)
    }

    pub fn do_edit_buffer(
        &mut self,
        cursor: &mut Cursor,
        cmd: &EditCommand,
        modal: bool,
        register: &mut Register,
        smart_tab: bool,
    ) -> Result<Vec<(Rope, RopeDelta, InvalLines)>> {
        let mut rs = Vec::with_capacity(1);
        self.buffer_edit(EditBuffer::DoEditBuffer {
            cursor,
            cmd,
            modal,
            register,
            smart_tab,
            response: &mut rs,
        })?;
        Ok(rs)
    }

    pub fn do_insert_buffer(
        &mut self,
        cursor: &mut Cursor,
        s: &str,
    ) -> Result<Vec<(Rope, RopeDelta, InvalLines)>> {
        let mut rs = Vec::new();
        self.buffer_edit(EditBuffer::DoInsertBuffer {
            cursor,
            s,
            response: &mut rs,
        })?;
        Ok(rs)
    }

    pub fn clear_completion_lens(&mut self) {
        self.completion_lens = None;
        if let Err(err) = self.update_lines_new(OriginLinesDelta::default()) {
            error!("{err:?}")
        }
        self.on_update_lines();
        self.update_screen_lines();
        self.update_folding_display_items();
    }

    pub fn init_diagnostics(&mut self) -> Result<()> {
        self.init_diagnostics_with_buffer()?;
        self.update_lines_new(OriginLinesDelta::default())?;
        self.on_update_lines();
        self.update_screen_lines();
        self.update_folding_display_items();
        Ok(())
    }

    pub fn update_viewport_size(&mut self, viewport: Rect) -> Result<()> {
        let viewport_size = viewport.size();

        let should_update =
            matches!(self.editor_style.wrap_method(), WrapMethod::EditorWidth)
                && self.viewport_size.width != viewport_size.width;
        if should_update {
            self.viewport_size = viewport_size;
        }
        if self.signals.viewport.update_if_not_equal(viewport) {
            self.update_screen_lines();
            self.update_folding_display_items();
        }
        self.trigger_signals();
        Ok(())
    }

    pub fn update_viewport_by_scroll(&mut self, viewport: Rect) {
        debug!("viewport={viewport:?} self.signals.viewport={:?} {:?}", self.signals.viewport.val(), self.editor_style.wrap_method());
        if self.signals.viewport.val().y0 == viewport.y0 && self.signals.viewport.val().y1 == viewport.y1 && !matches!(self.editor_style.wrap_method(), WrapMethod::EditorWidth) {
            return;
        }
        if self.signals.viewport.update_if_not_equal(viewport) {
            self.update_screen_lines();
            self.update_folding_display_items();
            self.trigger_signals();
        }
    }

    pub fn update_config(&mut self, config: EditorConfig) -> Result<()> {
        if self.config != config {
            self.config = config;
            self.update_lines_new(OriginLinesDelta::default())?;
            self.on_update_lines();
            self.update_screen_lines();
            self.update_folding_display_items();
            self.trigger_signals();
        }
        Ok(())
    }

    pub fn update_folding_ranges(&mut self, action: UpdateFolding) -> Result<()> {
        match action {
            UpdateFolding::UpdateByItem(item) => {
                self.folding_ranges.update_folding_item(item);
            }
            UpdateFolding::New(ranges) => {
                self.folding_ranges.update_ranges(ranges);
            }
            UpdateFolding::UpdateByPhantom(position) => {
                self.folding_ranges.update_by_phantom(position);
            }
            UpdateFolding::FoldCode(offset) => {
                let rope = self.signals.buffer.val().text();
                self.folding_ranges.fold_by_offset(offset, rope)?;
            }
        }
        self.update_lines_new(OriginLinesDelta::default())?;
        self.check_lines();
        self.update_screen_lines();
        self.update_folding_display_items();
        self.trigger_signals();
        Ok(())
    }

    pub fn update_inline_completion(&mut self, delta: &RopeDelta) -> Result<()> {
        let Some((completion, ..)) = self.inline_completion.take() else {
            return Ok(());
        };
        let (line, col) = self.completion_pos;
        let offset = self.buffer().offset_of_line_col(line, col)?;

        // Shift the position by the rope delta
        let mut transformer = Transformer::new(delta);

        let new_offset = transformer.transform(offset, true);
        let new_pos = self.buffer().offset_to_line_col(new_offset)?;

        if delta.as_simple_insert().is_some() {
            let (iv, new_len) = delta.summary();
            if iv.start() == iv.end()
                && iv.start() == offset
                && new_len <= completion.len()
            {
                // Remove the # of newly inserted characters
                // These aren't necessarily the same as the characters
                // literally in the text, but the
                // completion will be updated when the completion
                // widget receives the update event,
                // and it will fix this if needed.
                self.inline_completion =
                    Some((completion[new_len..].to_string(), new_pos.0, new_pos.1));
            }
        } else {
            self.inline_completion = Some((completion, new_pos.0, new_pos.1));
        }
        self.update_lines_new(OriginLinesDelta::default())?;
        self.on_update_lines();
        self.update_screen_lines();
        self.update_folding_display_items();
        self.trigger_signals();
        Ok(())
    }

    pub fn apply_delta(&mut self, delta: &RopeDelta) -> Result<()> {
        if self.style_from_lsp {
            if let Some(styles) = &mut self.semantic_styles {
                styles.1.apply_shape(delta);
            }
        } else if let Some(styles) = self.syntax.styles.as_mut() {
            styles.apply_shape(delta);
        }
        self.syntax.lens.apply_delta(delta);
        self.update_diagnostics(delta);
        self.update_inlay_hints(delta);
        self.update_completion_lens(delta)?;
        // self.update_lines();
        self.on_update_lines();
        self.update_screen_lines();
        self.update_folding_display_items();
        self.trigger_signals();
        Ok(())
    }

    pub fn trigger_syntax_change(
        &mut self,
        _edits: Option<SmallVec<[SyntaxEdit; 3]>>,
    ) -> Result<()> {
        self.syntax.cancel_flag.store(1, atomic::Ordering::Relaxed);
        self.syntax.cancel_flag = Arc::new(AtomicUsize::new(0));
        self.update_lines_new(OriginLinesDelta::default())?;
        self.on_update_lines();
        self.update_screen_lines();
        self.update_folding_display_items();
        self.trigger_signals();
        Ok(())
    }

    pub fn set_inline_completion(
        &mut self,
        inline_completion: String,
        line: usize,
        col: usize,
    ) -> Result<()> {
        self.inline_completion = Some((inline_completion, line, col));
        self.update_lines_new(OriginLinesDelta::default())?;
        self.on_update_lines();
        self.update_screen_lines();
        self.update_folding_display_items();
        self.trigger_signals();
        Ok(())
    }

    pub fn clear_inline_completion(&mut self) -> Result<()> {
        self.inline_completion = None;
        self.update_lines_new(OriginLinesDelta::default())?;
        self.on_update_lines();
        self.update_screen_lines();
        self.update_folding_display_items();
        self.trigger_signals();
        Ok(())
    }

    pub fn set_syntax_with_rev(&mut self, syntax: Syntax, rev: u64) -> Result<bool> {
        if self.buffer().rev() != rev {
            return Ok(false);
        }
        self.set_syntax(syntax)
    }

    pub fn set_syntax(&mut self, syntax: Syntax) -> Result<bool> {
        self.syntax = syntax;
        if self.style_from_lsp {
            return Ok(false);
        }
        self.update_parser()?;

        self.update_lines_new(OriginLinesDelta::default())?;
        self.on_update_lines();
        self.update_screen_lines();
        self.update_folding_display_items();
        self.trigger_signals();
        Ok(true)
    }

    pub fn set_inlay_hints(&mut self, inlay_hint: Spans<InlayHint>) -> Result<()> {
        self.inlay_hints = Some(inlay_hint);
        self.update_lines_new(OriginLinesDelta::default())?;
        self.on_update_lines();
        self.update_screen_lines();
        self.update_folding_display_items();
        self.trigger_signals();
        Ok(())
    }

    pub fn set_completion_lens(
        &mut self,
        completion_lens: String,
        line: usize,
        col: usize,
    ) -> Result<()> {
        self.completion_lens = Some(completion_lens);
        self.completion_pos = (line, col);
        self.update_lines_new(OriginLinesDelta::default())?;
        self.on_update_lines();
        self.update_screen_lines();
        self.update_folding_display_items();
        self.trigger_signals();
        Ok(())
    }

    pub fn update_semantic_styles_from_lsp(
        &mut self,
        styles: (Option<String>, Spans<String>),
        rev: u64,
    ) -> Result<bool> {
        if self.buffer().rev() != rev {
            return Ok(false);
        }
        self.style_from_lsp = true;
        self.semantic_styles = Some(styles);
        self.update_lines_new(OriginLinesDelta::default())?;
        self.on_update_lines();
        self.update_screen_lines();
        self.update_folding_display_items();
        self.trigger_signals();
        Ok(true)
    }
}

type LinesEditorStyle = DocLines;

impl LinesEditorStyle {
    pub fn modal(&self) -> bool {
        self.editor_style.modal()
    }

    pub fn current_line_color(&self) -> Option<Color> {
        EditorStyle::current_line(&self.editor_style)
    }

    pub fn scroll_beyond_last_line(&self) -> bool {
        EditorStyle::scroll_beyond_last_line(&self.editor_style)
    }

    pub fn ed_caret(&self) -> Brush {
        self.editor_style.ed_caret()
    }

    pub fn selection_color(&self) -> Color {
        self.editor_style.selection()
    }

    pub fn indent_style(&self) -> IndentStyle {
        self.editor_style.indent_style()
    }

    pub fn indent_guide(&self) -> Color {
        self.editor_style.indent_guide()
    }

    pub fn visible_whitespace(&self) -> Color {
        self.editor_style.visible_whitespace()
    }

    pub fn update_editor_style(&mut self, cx: &mut StyleCx<'_>) -> Result<bool> {
        // todo
        let updated = self.editor_style.read(cx);
        let new_show_indent_guide = self.show_indent_guide();
        self.signals
            .show_indent_guide
            .update_if_not_equal(new_show_indent_guide);
        if updated {
            self.update_lines_new(OriginLinesDelta::default())?;
        }
        self.trigger_signals();
        Ok(updated)
    }

    pub fn show_indent_guide(&self) -> (bool, Color) {
        (
            self.editor_style.show_indent_guide(),
            self.editor_style.indent_guide()
        )
    }
}

#[allow(dead_code)]
/// 以界面为单位，进行触发。
type LinesSignals = DocLines;

#[allow(dead_code)]
/// 以界面为单位，进行触发。
impl LinesSignals {
    pub fn signal_viewport(&self) -> ReadSignal<Rect> {
        self.signals.viewport.signal()
    }

    pub fn signal_show_indent_guide(&self) -> ReadSignal<(bool, Color)> {
        self.signals.show_indent_guide.signal()
    }

    pub fn signal_screen_lines(&self) -> ReadSignal<ScreenLines> {
        self.signals.screen_lines.signal()
    }

    pub fn signal_folding_items(&self) -> ReadSignal<Vec<FoldingDisplayItem>> {
        self.signals.folding_items.signal()
    }

    pub fn signal_buffer_rev(&self) -> ReadSignal<u64> {
        self.signals.signal_buffer_rev()
    }

    pub fn signal_buffer(&self) -> ReadSignal<Buffer> {
        self.signals.buffer.signal()
    }

    pub fn signal_last_line(&self) -> ReadSignal<(usize, f64)> {
        self.signals.last_line.signal()
    }

    pub fn signal_pristine(&self) -> ReadSignal<bool> {
        self.signals.pristine.signal()
    }
}

type LinesProperty = DocLines;

impl LinesProperty {
    pub fn screen_lines(&self) -> &ScreenLines {
        self.signals.screen_lines.val()
    }
}

pub trait RopeTextPosition: RopeText {
    /// Converts a UTF8 offset to a UTF16 LSP position
    /// Returns None if it is not a valid UTF16 offset
    fn offset_to_position(&self, offset: usize) -> Result<Position> {
        let (line, col) = self.offset_to_line_col(offset)?;
        let line_offset = self.offset_of_line(line)?;

        let utf16_col =
            offset_utf8_to_utf16(self.char_indices_iter(line_offset..), col);

        Ok(Position {
            line: line as u32,
            character: utf16_col as u32,
        })
    }

    fn offset_of_position(&self, pos: &Position) -> Result<usize> {
        let (line, column) = self.position_to_line_col(pos)?;

        self.offset_of_line_col(line, column)
    }

    fn position_to_line_col(&self, pos: &Position) -> Result<(usize, usize)> {
        let line = pos.line as usize;
        let line_offset = self.offset_of_line(line)?;

        let column = offset_utf16_to_utf8(
            self.char_indices_iter(line_offset..),
            pos.character as usize,
        );

        Ok((line, column))
    }
}

impl<T: RopeText> RopeTextPosition for T {}

#[derive(Debug)]
pub enum ClickResult {
    NoHint,
    MatchWithoutLocation,
    MatchFolded,
    MatchHint(Location),
}
