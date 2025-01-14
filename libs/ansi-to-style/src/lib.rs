use std::collections::HashMap;
use std::ptr::NonNull;
use peniko::Color;
use vte::{Params, Parser, Perform};

#[derive(Debug, Default, Clone)]
pub struct StyledText {
    pub text: String,
    pub styles: Vec<TextStyle>,
}

#[derive(Debug, Default, Clone)]
pub struct TextStyle {
    start: usize,
    end: usize,
    bold: bool,
    italic: bool,
    underline: bool,
    bg_color: Option<Color>,
    fg_color: Option<Color>
}

enum StyleState {
    None,
    Init {
        bold: bool,
        italic: bool,
        underline: bool,
        bg_color: Option<Color>,
        fg_color: Option<Color>
    },
    Ref {
        start: usize,
        end: usize,
        bold: bool,
        italic: bool,
        underline: bool,
        bg_color: Option<Color>,
        fg_color: Option<Color>
    }
}

impl StyleState {
    pub fn ref_by(&mut self, offset: usize) {
        let update_state = match self {
            StyleState::None => {return;}
            StyleState::Init {
                bold, italic, underline, bg_color, fg_color
            } => {
                Self::Ref {
                    start: offset,
                    end: offset,
                    bold: *bold,
                    italic: *italic,
                    underline: *underline,
                    bg_color: bg_color.clone(),
                    fg_color: fg_color.clone(),
                }
            }
            StyleState::Ref { end, .. } => {
                *end = offset;
                return;
            }
        };
        *self = update_state;
    }

    pub fn init(&mut self, new_bold: Option<bool>, new_italic: Option<bool>, new_underline: Option<bool>, new_bg_color: Option<Color>,
                new_fg_color: Option<Color>) -> Option<TextStyle>{
        let (update_state, style) = match self {
            StyleState::None => {
                let bold = new_bold.unwrap_or_default();
                let italic = new_italic.unwrap_or_default();
                let underline = new_underline.unwrap_or_default();
                (Self::Init {
                    bold,
                    italic,
                    underline,
                    bg_color: new_bg_color,
                    fg_color: new_fg_color,
                }, None)
            }
            StyleState::Init {
                bold, italic, underline, bg_color, fg_color
            } => {
                if let Some(new_bold) = new_bold {
                    *bold = new_bold;
                }
                if let Some(new_italic) = new_italic {
                    *italic = new_italic;
                }
                if let Some(new_underline) = new_underline {
                    *underline = new_underline;
                }
                if let Some(new_bg_color) = new_bg_color {
                    *bg_color = Some(new_bg_color);
                }
                if let Some(new_fg_color) = new_fg_color {
                    *fg_color = Some(new_fg_color);
                }
                return None;
            }
            StyleState::Ref { start, end, bold, italic, underline, bg_color, fg_color } => {
                let style = TextStyle {
                    start: *start,
                    end: *end,
                    bold: *bold,
                    italic: *italic,
                    underline: *underline,
                    bg_color: *bg_color,
                    fg_color: *fg_color,
                };
                let bold = new_bold.unwrap_or(*bold);
                let italic = new_italic.unwrap_or(*italic);
                let underline = new_underline.unwrap_or(*underline);
                let bg_color = if new_bg_color.is_none() {
                    *bg_color
                } else {
                    new_bg_color
                };
                let fg_color = if new_fg_color.is_none() {
                    *fg_color
                } else {
                    new_fg_color
                };
                (Self::Init {
                    bold,
                    italic,
                    underline,
                    bg_color,
                    fg_color,
                }, Some(style))
            }
        };
        *self = update_state;
        style
    }
    pub fn clear(&mut self) -> Option<TextStyle>{
        let (update_state, style) = match self {
            StyleState::None => {
                return None;
            }
            StyleState::Init {
                ..
            } => {
                (Self::None, None)
            }
            StyleState::Ref { start, end, bold, italic, underline, bg_color, fg_color } => {
                (Self::None, Some(TextStyle {
                    start: *start,
                    end: *end,
                    bold: *bold,
                    italic: *italic,
                    underline: *underline,
                    bg_color: *bg_color,
                    fg_color: *fg_color,
                }))
            }
        };
        *self = update_state;
        style
    }
}

struct TerminalParser {
    output: StyledText,
    style_state: StyleState
}

impl TerminalParser {
    fn new() -> Self {
        Self {
            output: StyledText::default(),
            style_state: StyleState::None,
        }
    }
}

impl Perform for TerminalParser {
    fn print(&mut self, c: char) {
        self.output.text.push(c);
        self.style_state.ref_by(self.output.text.len() - 1);
    }

    fn execute(&mut self, byte: u8) {
        if byte == b'\n' {
            self.output.text.push('\n');
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: char,
    ) {
        if _action != 'm' {
            return; // 只处理 SGR (m) 操作
        }

        // 将参数展开为一个扁平化的迭代器
        let mut flat_params = params.iter().flat_map(|sub_params| sub_params.iter());
        while let Some(&param) = flat_params.next() {
            match param {
                0 => if let Some(style) = self.style_state.clear() {
                    self.output.styles.push(style);
                }, // 重置样式
                1 => {
                    if let Some(style) = self.style_state.init(Some(true), None, None, None, None) {
                        self.output.styles.push(style);
                    }
                }
                3 => {
                    if let Some(style) = self.style_state.init(None, Some(true),  None, None, None) {
                        self.output.styles.push(style);
                    }
                }
                4 => {
                    if let Some(style) = self.style_state.init(None, None, Some(true), None, None) {
                        self.output.styles.push(style);
                    }
                }
                30..=37 => {
                    // 标准前景色
                    let color = match param {
                        30 => Color::BLACK,
                        31 => Color::RED,
                        32 => Color::GREEN,
                        33 => Color::YELLOW,
                        34 => Color::BLUE,
                        35 => Color::MAGENTA,
                        36 => Color::CYAN,
                        37 => Color::WHITE,
                        _ => continue,
                    };
                    if let Some(style) = self.style_state.init(None, None, None, None, Some(color)) {
                        self.output.styles.push(style);
                    }
                }
                38 => {
                    // 扩展前景色 (RGB 模式)
                    if let (Some(&2), Some(&r), Some(&g), Some(&b)) = (
                        flat_params.next(),
                        flat_params.next(),
                        flat_params.next(),
                        flat_params.next(),
                    ) {
                        if let Some(style) = self.style_state.init(None, None, None, None, Some(Color::rgb(r as f64, g as f64, b as f64))) {
                            self.output.styles.push(style);
                        }
                    }
                }
                40..=47 => {
                    // 标准背景色
                    let color = match param {
                        40 => Color::BLACK,
                        41 => Color::RED,
                        42 => Color::GREEN,
                        43 => Color::YELLOW,
                        44 => Color::BLUE,
                        45 => Color::MAGENTA,
                        46 => Color::CYAN,
                        47 => Color::WHITE,
                        _ => continue,
                    };
                    if let Some(style) = self.style_state.init(None, None, None, Some(color), None) {
                        self.output.styles.push(style);
                    }
                }
                48 => {
                    // 扩展背景色 (RGB 模式)
                    if let (Some(&2), Some(&r), Some(&g), Some(&b)) = (
                        flat_params.next(),
                        flat_params.next(),
                        flat_params.next(),
                        flat_params.next(),
                    ) {
                        if let Some(style) = self.style_state.init(None, None, None, Some(Color::rgb(r as f64, g as f64, b as f64)), None) {
                            self.output.styles.push(style);
                        }
                    }
                }
                _ => {} // 忽略未处理的参数
            }
        }
    }
}

pub fn parse_byte(input: &[u8]) -> StyledText {
    let mut parser = Parser::new();
    let mut handler = TerminalParser::new();
    parser.advance(&mut handler, input);
    handler.output
}
