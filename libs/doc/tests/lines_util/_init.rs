use std::{
    fs::File,
    path::{Path, PathBuf}
};

use anyhow::Result;
use doc::{
    DiagnosticData, EditorViewKind,
    config::EditorConfig,
    language::LapceLanguage,
    lines::{
        DocLines, RopeTextPosition,
        buffer::{Buffer, rope_text::RopeText},
        fold::{FoldingDisplayItem, FoldingDisplayType, FoldingRange}
    },
    syntax::{BracketParser, Syntax}
};
use floem::{
    kurbo::Rect,
    reactive::{RwSignal, Scope, SignalUpdate},
};
use itertools::Itertools;
use lapce_xi_rope::{
    Interval,
    spans::{Spans, SpansBuilder}
};
use log::info;
use lsp_types::{Diagnostic, InlayHint, Position};
use doc::lines::cursor::{Cursor, CursorMode};
use doc::lines::selection::Selection;
use doc::lines::style::EditorStyle;

use crate::lines_util::init_semantic_2;

fn _init_lsp_folding_range() -> Vec<FoldingRange> {
    let folding_range = r#"[{"startLine":0,"startCharacter":10,"endLine":7,"endCharacter":1},{"startLine":1,"startCharacter":12,"endLine":3,"endCharacter":5},{"startLine":3,"startCharacter":11,"endLine":5,"endCharacter":5}]"#;
    let folding_range: Vec<lsp_types::FoldingRange> =
        serde_json::from_str(folding_range).unwrap();

    folding_range
        .into_iter()
        .map(FoldingRange::from_lsp)
        .sorted_by(|x, y| x.start.line.cmp(&y.start.line))
        .collect()
}

fn _init_lsp_folding_range_2() -> Vec<FoldingRange> {
    let folding_range = r#"[{"startLine":0,"startCharacter":10,"endLine":7,"endCharacter":1},{"startLine":1,"startCharacter":12,"endLine":3,"endCharacter":5},{"startLine":3,"startCharacter":11,"endLine":5,"endCharacter":5},{"startLine":10,"startCharacter":10,"endLine":27,"endCharacter":1}]"#;
    let folding_range: Vec<lsp_types::FoldingRange> =
        serde_json::from_str(folding_range).unwrap();

    folding_range
        .into_iter()
        .map(FoldingRange::from_lsp)
        .sorted_by(|x, y| x.start.line.cmp(&y.start.line))
        .collect()
}

fn _init_inlay_hint(buffer: &Buffer) -> Result<Spans<InlayHint>> {
    let hints = r#"[{"position":{"line":6,"character":9},"label":[{"value":": "},{"value":"A","location":{"uri":"file:///d:/git/check/src/main.rs","range":{"start":{"line":8,"character":7},"end":{"line":8,"character":8}}}}],"kind":1,"textEdits":[{"range":{"start":{"line":6,"character":9},"end":{"line":6,"character":9}},"newText":": A"}],"paddingLeft":false,"paddingRight":false}]"#;
    let mut hints: Vec<InlayHint> = serde_json::from_str(hints).unwrap();
    let len = buffer.len();
    hints.sort_by(|left, right| left.position.cmp(&right.position));
    let mut hints_span = SpansBuilder::new(len);
    for hint in hints {
        let offset = buffer.offset_of_position(&hint.position)?.min(len);
        hints_span.add_span(Interval::new(offset, (offset + 1).min(len)), hint);
    }
    Ok(hints_span.build())
}

fn _init_code(file: PathBuf) -> (String, Buffer) {
    // let code = "pub fn main() {\r\n    if true {\r\n
    // println!(\"startss\");\r\n    } else {\r\n
    // println!(\"startss\");\r\n    }\r\n    let a =
    // A;\r\n}\r\nstruct A;\r\n";
    let code = load_code(&file);
    let buffer = Buffer::new(code.as_str());
    info!("line_ending {:?} len={}", buffer.line_ending(), code.len());
    (code, buffer)
}

///  2|   if true {...} else {\r\n
pub fn folded_v1() -> FoldingDisplayItem {
    FoldingDisplayItem {
        position: Position {
            line:      1,
            character: 12
        },
        y:        0,
        ty:       FoldingDisplayType::UnfoldStart
    }
}

///  2|   if true {...} else {...}\r\n
pub fn folded_v2() -> FoldingDisplayItem {
    FoldingDisplayItem {
        position: Position {
            line:      5,
            character: 5
        },
        y:        0,
        ty:       FoldingDisplayType::UnfoldEnd
    }
}

fn _init_lines(
    folded: Option<Vec<FoldingDisplayItem>>,
    (code, buffer): (String, Buffer),
    folding: Vec<FoldingRange>
) -> Result<(DocLines, EditorConfig)> {
    // let folding = _init_lsp_folding_range();
    let hints = _init_inlay_hint(&buffer)?;

    let config_str = r##"{"auto_closing_matching_pairs":true, "auto_surround":true,"font_family":"JetBrains Mono","font_size":13,"line_height":23,"enable_inlay_hints":true,"inlay_hint_font_size":0,"enable_error_lens":true,"error_lens_end_of_line":true,"error_lens_multiline":false,"error_lens_font_size":0,"enable_completion_lens":false,"enable_inline_completion":true,"completion_lens_font_size":0,"only_render_error_styling":false,"diagnostic_error":{"r":229,"g":20,"b":0,"a":255},"diagnostic_warn":{"r":233,"g":167,"b":0,"a":255},"inlay_hint_fg":{"r":108,"g":118,"b":128,"a":255},"inlay_hint_bg":{"r":245,"g":245,"b":245,"a":255},"error_lens_error_foreground":{"r":228,"g":86,"b":73,"a":255},"error_lens_warning_foreground":{"r":193,"g":132,"b":1,"a":255},"error_lens_other_foreground":{"r":160,"g":161,"b":167,"a":255},"completion_lens_foreground":{"r":160,"g":161,"b":167,"a":255},"editor_foreground":{"r":56,"g":58,"b":66,"a":255},"syntax":{"punctuation.delimiter":{"r":193,"g":132,"b":1,"a":255},"attribute":{"r":193,"g":132,"b":1,"a":255},"method":{"r":64,"g":120,"b":242,"a":255},"bracket.color.3":{"r":166,"g":38,"b":164,"a":255},"builtinType":{"r":18,"g":63,"b":184,"a":255},"enumMember":{"r":146,"g":17,"b":167,"a":255},"bracket.color.2":{"r":193,"g":132,"b":1,"a":255},"markup.heading":{"r":228,"g":86,"b":73,"a":255},"markup.link.url":{"r":64,"g":120,"b":242,"a":255},"string.escape":{"r":1,"g":132,"b":188,"a":255},"structure":{"r":193,"g":132,"b":1,"a":255},"text.reference":{"r":193,"g":132,"b":1,"a":255},"comment":{"r":160,"g":161,"b":167,"a":255},"markup.list":{"r":209,"g":154,"b":102,"a":255},"variable.other.member":{"r":228,"g":86,"b":73,"a":255},"type":{"r":56,"g":58,"b":66,"a":255},"keyword":{"r":7,"g":60,"b":183,"a":255},"text.uri":{"r":1,"g":132,"b":188,"a":255},"enum":{"r":56,"g":58,"b":66,"a":255},"constructor":{"r":193,"g":132,"b":1,"a":255},"interface":{"r":56,"g":58,"b":66,"a":255},"selfKeyword":{"r":166,"g":38,"b":164,"a":255},"type.builtin":{"r":1,"g":132,"b":188,"a":255},"escape":{"r":1,"g":132,"b":188,"a":255},"field":{"r":228,"g":86,"b":73,"a":255},"function.method":{"r":64,"g":120,"b":242,"a":255},"markup.link.text":{"r":166,"g":38,"b":164,"a":255},"property":{"r":136,"g":22,"b":150,"a":255},"struct":{"r":56,"g":58,"b":66,"a":255},"bracket.color.1":{"r":64,"g":120,"b":242,"a":255},"enum-member":{"r":228,"g":86,"b":73,"a":255},"string":{"r":80,"g":161,"b":79,"a":255},"text.title":{"r":209,"g":154,"b":102,"a":255},"bracket.unpaired":{"r":228,"g":86,"b":73,"a":255},"constant":{"r":193,"g":132,"b":1,"a":255},"typeAlias":{"r":56,"g":58,"b":66,"a":255},"function":{"r":61,"g":108,"b":126,"a":255},"markup.link.label":{"r":166,"g":38,"b":164,"a":255},"markup.bold":{"r":209,"g":154,"b":102,"a":255},"markup.italic":{"r":209,"g":154,"b":102,"a":255},"number":{"r":193,"g":132,"b":1,"a":255},"tag":{"r":64,"g":120,"b":242,"a":255},"variable":{"r":56,"g":58,"b":66,"a":255},"embedded":{"r":1,"g":132,"b":188,"a":255}}}"##;
    let config: EditorConfig = serde_json::from_str(config_str).unwrap();
    let cx = Scope::new();
    let diagnostics = DiagnosticData {
        expanded:         cx.create_rw_signal(false),
        diagnostics:      cx.create_rw_signal(im::Vector::new()),
        diagnostics_span: cx.create_rw_signal(Spans::default())
    };
    // { x0: 0.0, y0: 0.0, x1: 591.1680297851563, y1:
    // 538.1586303710938 }
    let view = Rect::new(0.0, 0.0, 591.0, 538.0);
    let editor_style = EditorStyle::default();
    let kind = cx.create_rw_signal(EditorViewKind::Normal);
    let language = LapceLanguage::Rust;
    let grammars_dir: PathBuf = "C:\\Users\\36225\\AppData\\Local\\lapce\\\
                                 Lapce-Debug\\data\\grammars"
        .into();

    let queries_directory: PathBuf = "C:\\Users\\36225\\AppData\\Roaming\\lapce\\\
                                      Lapce-Debug\\config\\queries"
        .into();

    let syntax = Syntax::from_language(language, &grammars_dir, &queries_directory);
    let parser = BracketParser::new(code.to_string(), true, 30000);
    let mut lines = DocLines::new(
        cx,
        diagnostics,
        syntax,
        parser,
        view,
        editor_style,
        config.clone(),
        buffer,
        kind
    )?;
    lines.update_folding_ranges(folding.into())?;
    lines.set_inlay_hints(hints)?;
    if let Some(folded) = folded {
        for folded in folded {
            lines.update_folding_ranges(folded.into())?;
        }
    }
    Ok((lines, config))
}

fn load_code(file: &Path) -> String {
    std::fs::read_to_string(file).unwrap()
}

/// main_2.rs
fn init_diag_2() -> im::Vector<Diagnostic> {
    let mut diags = im::Vector::new();
    diags.push_back(serde_json::from_str(r#"{"range":{"start":{"line":6,"character":8},"end":{"line":6,"character":9}},"severity":2,"code":"unused_variables","source":"rustc","message":"unused variable: `a`\n`#[warn(unused_variables)]` on by default","relatedInformation":[{"location":{"uri":"file:///d:/git/check/src/main.rs","range":{"start":{"line":6,"character":8},"end":{"line":6,"character":9}}},"message":"if this is intentional, prefix it with an underscore: `_a`"}],"tags":[1],"data":{"rendered":"warning: unused variable: `a`\n --> src/main.rs:7:9\n  |\n7 |     let a = A;\n  |         ^ help: if this is intentional, prefix it with an underscore: `_a`\n  |\n  = note: `#[warn(unused_variables)]` on by default\n\n"}}"#).unwrap());
    diags.push_back(serde_json::from_str(r#"{"range":{"start":{"line":6,"character":8},"end":{"line":6,"character":9}},"severity":4,"code":"unused_variables","source":"rustc","message":"if this is intentional, prefix it with an underscore: `_a`","relatedInformation":[{"location":{"uri":"file:///d:/git/check/src/main.rs","range":{"start":{"line":6,"character":8},"end":{"line":6,"character":9}}},"message":"original diagnostic"}]}"#).unwrap());
    diags.push_back(serde_json::from_str(r#"{"range":{"start":{"line":10,"character":3},"end":{"line":10,"character":7}},"severity":2,"code":"dead_code","source":"rustc","message":"function `test` is never used\n`#[warn(dead_code)]` on by default","tags":[1],"data":{"rendered":"warning: function `test` is never used\n  --> src/main.rs:11:4\n   |\n11 | fn test() {\n   |    ^^^^\n   |\n   = note: `#[warn(dead_code)]` on by default\n\n"}}"#).unwrap());
    diags
}

pub fn init_main_2() -> Result<DocLines> {
    custom_utils::logger::logger_stdout_debug();
    let file: PathBuf = "resources/test_code/main_2.rs".into();

    let folding = _init_lsp_folding_range_2();
    let (mut lines, _) = _init_lines(None, _init_code(file), folding)?;
    let diags = init_diag_2();
    let semantic = init_semantic_2();

    lines.diagnostics.diagnostics.update(|x| *x = diags);
    lines.init_diagnostics()?;

    let mut styles_span = SpansBuilder::new(lines.buffer().len());
    for style in semantic.styles {
        if let Some(fg) = style.style.fg_color {
            styles_span.add_span(Interval::new(style.start, style.end), fg);
        }
    }
    let styles = styles_span.build();

    lines.update_semantic_styles_from_lsp((None, styles), lines.buffer().rev())?;

    Ok(lines)
}

pub fn init_main() -> Result<DocLines> {
    custom_utils::logger::logger_stdout_debug();
    let file: PathBuf = "resources/test_code/main.rs".into();
    let (lines, _) = _init_lines(None, _init_code(file), vec![])?;
    Ok(lines)
}

pub fn init_empty() -> Result<DocLines> {
    custom_utils::logger::logger_stdout_debug();
    let file: PathBuf = "resources/test_code/empty.rs".into();

    let (lines, _) = _init_lines(None, _init_code(file), _init_lsp_folding_range())?;
    Ok(lines)
}

pub fn cursor_insert(start: usize, end: usize) -> Cursor {
    let mode = CursorMode::Insert(Selection::region(start, end));
    Cursor::new(mode, None, None)
}