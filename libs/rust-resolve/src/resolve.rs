use crate::{ExtChannel, PanelStyle};
use cargo_metadata::{CompilerMessage, diagnostic::DiagnosticLevel};
use cozy_floem::data::{Hyperlink, Line};
use floem::{
    prelude::Color,
    text::{Attrs, AttrsList, FamilyOwned, LineHeightValue, Weight}
};
use std::ops::Range;

pub fn resolve_compiler_message<'a>(
    msg: &'a CompilerMessage,
    style: &PanelStyle,
    ext_channel: &mut ExtChannel<Line>
) {
    let mut lines = msg.message.message.lines();
    let family = style.font_family();

    let Some(title) = lines.next() else { return };
    let rs = resolve_title(
        title,
        &msg,
        style.font_size(),
        &family,
        style.line_height()
    );
    ext_channel.send(Line {
        content:    title.to_string(),
        attrs_list: rs.1,
        hyperlink:  rs.0
    });
    let Some(path) = lines.next() else { return };
    let rs = resolve_path(
        path,
        style.font_size(),
        &family,
        style.line_height()
    );
    ext_channel.send(Line {
        content:    path.to_string(),
        attrs_list: rs.1,
        hyperlink:  rs.0
    });
    while let Some(code) = lines.next() {
        let attrs_list = resolve_detail(
            code,
            msg.message.level,
            style.font_size(),
            &family,
            style.line_height(),
            Color::RED
        );
        ext_channel.send(Line {
            content: code.to_string(),
            attrs_list,
            hyperlink: vec![]
        });
    }
}

fn resolve_detail(
    msg: &str,
    diagnostic_level: DiagnosticLevel,
    font_size: f32,
    family: &[FamilyOwned],
    line_height: f32,
    split_color: Color
) -> AttrsList {
    let head = resolve_detail_head(msg);
    let attrs = Attrs::new()
        .family(family)
        .font_size(font_size)
        .line_height(LineHeightValue::Px(line_height));
    let mut attr_list = AttrsList::new(attrs);
    match head {
        Head::CodeMsg { split_index } => {
            let attrs = Attrs::new().color(split_color);
            attr_list.add_span(0..split_index, attrs);
            let attrs = Attrs::new()
                .color(level_color(diagnostic_level))
                .weight(Weight::BOLD);
            attr_list.add_span(split_index..msg.len(), attrs);
        },
        Head::Code { split_index } => {
            let attrs = Attrs::new().color(split_color);
            attr_list.add_span(0..split_index, attrs);
        },
        Head::Note {
            split_index,
            note_range
        } => {
            let attrs = Attrs::new().color(split_color);
            attr_list.add_span(0..split_index, attrs);
            let attrs = Attrs::new().weight(Weight::BOLD);
            attr_list.add_span(note_range, attrs);
        },
        Head::Other => {}
    }
    attr_list
}

fn resolve_path(
    msg: &str,
    font_size: f32,
    family: &[FamilyOwned],
    line_height: f32
) -> (Vec<Hyperlink>, AttrsList) {
    let mut links = Vec::new();
    let attrs = Attrs::new()
        .family(family)
        .font_size(font_size)
        .line_height(LineHeightValue::Px(line_height));
    let mut attr_list = AttrsList::new(attrs);
    let arrow = "-->";
    if let Some(range) = msg.find(arrow).map(|x| x..x + arrow.len()) {
        if let Some(index) =
            first_non_whitespace_index(&msg[range.end..])
        {
            links.push(Hyperlink {
                start_offset: range.end + index,
                end_offset:   msg.len(),
                // todo
                link:         "".to_string(),
                line_color:   Color::RED
            });
        }
        add_arrow_attrs(Color::BLUE, &mut attr_list, range);
    }
    (links, attr_list)
}

fn resolve_title(
    msg: &str,
    compiler_message: &CompilerMessage,
    font_size: f32,
    family: &[FamilyOwned],
    line_height: f32
) -> (Vec<Hyperlink>, AttrsList) {
    let mut links = Vec::new();
    let attrs = Attrs::new()
        .family(family)
        .font_size(font_size)
        .weight(Weight::BOLD)
        .line_height(LineHeightValue::Px(line_height));
    let mut attr_list = AttrsList::new(attrs);
    // add level style
    if let Some(level_str) = level_str(compiler_message.message.level)
    {
        if let Some(range) =
            msg.find(level_str).map(|x| x..x + level_str.len())
        {
            add_level_attrs(
                compiler_message.message.level,
                &mut attr_list,
                range,
                font_size,
                family,
                line_height
            );
        }
    }
    // add code style
    if let Some(code) =
        compiler_message.message.code.as_ref().map(|x| &x.code)
    {
        if let Some(range) = msg.find(code).map(|x| x..x + code.len())
        {
            links.push(Hyperlink {
                start_offset: range.start,
                end_offset:   range.end,
                // todo
                link:         "".to_string(),
                line_color:   level_color(
                    compiler_message.message.level
                )
            });
            add_level_attrs(
                compiler_message.message.level,
                &mut attr_list,
                range,
                font_size,
                family,
                line_height
            );
        }
    }
    (links, attr_list)
}

enum Head {
    ///   |     ^ not found in this scope
    CodeMsg {
        split_index: usize
    },
    ///15 |     a
    Code {
        split_index: usize
    },
    ///   = note: `#[warn(unused_variables)]` on by default
    Note {
        split_index: usize,
        note_range:  Range<usize>
    },
    Other
}

fn resolve_detail_head(msg: &str) -> Head {
    if let Some(mut split_index) = msg.find('|') {
        split_index += 1;
        if find_num_index(&msg[0..split_index]) {
            Head::Code { split_index }
        } else {
            Head::CodeMsg { split_index }
        }
    } else if let Some(index) = msg.find("= note") {
        Head::Note {
            split_index: index + 1,
            note_range:  Range {
                start: index + 2,
                end:   index + 6
            }
        }
    } else {
        Head::Other
    }
}

fn add_arrow_attrs(
    color: Color,
    attr_list: &mut AttrsList,
    range: Range<usize>
) {
    let attrs = Attrs::new().color(color);
    attr_list.add_span(range, attrs);
}

fn add_level_attrs(
    diagnostic_level: DiagnosticLevel,
    attr_list: &mut AttrsList,
    range: Range<usize>,
    font_size: f32,
    family: &[FamilyOwned],
    line_height: f32
) {
    let attrs = Attrs::new()
        .color(level_color(diagnostic_level))
        .family(family)
        .font_size(font_size)
        // .weight(Weight::BOLD)
        .line_height(LineHeightValue::Px(line_height));
    attr_list.add_span(range, attrs);
}

fn first_non_whitespace_index(s: &str) -> Option<usize> {
    s.chars()
        .enumerate()
        .find(|&(_, c)| !c.is_whitespace())
        .map(|(i, _)| i)
}

fn find_num_index(s: &str) -> bool {
    s.chars().find(|&c| c.is_digit(10)).is_some()
}

#[inline]
fn level_color(level: DiagnosticLevel) -> Color {
    match level {
        DiagnosticLevel::Ice => Color::RED,
        DiagnosticLevel::Error => Color::RED,
        DiagnosticLevel::Warning => Color::YELLOW,
        _ => Color::YELLOW
    }
}

fn level_str(level: DiagnosticLevel) -> Option<&'static str> {
    Some(match level {
        DiagnosticLevel::Ice => "ice",
        DiagnosticLevel::Error => "error",
        DiagnosticLevel::Warning => "warning",
        DiagnosticLevel::FailureNote => "failure note",
        DiagnosticLevel::Note => "note",
        DiagnosticLevel::Help => "help",
        _ => return None
    })
}
