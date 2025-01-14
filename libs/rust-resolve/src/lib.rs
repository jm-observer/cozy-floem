mod resolve;

use crate::resolve::resolve_compiler_message;
use anyhow::Result;
use cargo_metadata::Message;
use cozy_floem::data::{Hyperlink, Line, ranges_overlap, Styled};
use floem::{
    ext_event::{
        ExtSendTrigger, create_ext_action, register_ext_trigger,
    },
    prelude::{SignalGet, SignalUpdate},
    reactive::{ReadSignal, Scope, with_scope},
    text::{
        Attrs, AttrsList, FamilyOwned, LineHeightValue,
    },
};
use log::{debug, error};
use parking_lot::Mutex;
use std::{collections::VecDeque, sync::Arc};
use std::ops::Range;
use floem::prelude::Color;
use floem::text::{Style, Weight};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::mpsc,
};
use ansi_to_style::{parse_byte, TextStyle};

pub enum OutputLine {
    StdOut(String),
    StdErr(String),
}

pub async fn run_command(
    mut command: Command,
    mut channel: ExtChannel<StyledText>,
    style: PanelStyle,
) -> Result<()> {
    // 启动子进程，并捕获 stdout 和 stderr
    let mut child = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to start cargo build");

    let (tx, mut rx) = mpsc::channel(100);

    // 异步读取 stdout
    if let Some(stdout) = child.stdout.take() {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if tx.send(OutputLine::StdOut(line)).await.is_err() {
                    break;
                }
            }
        });
    }

    // 异步读取 stderr
    if let Some(stderr) = child.stderr.take() {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if tx.send(OutputLine::StdErr(line)).await.is_err() {
                    break;
                }
            }
        });
    }

    let font_family = style.font_family();
    let attrs = Attrs::new()
        .family(&font_family)
        .font_size(style.font_size)
        .line_height(LineHeightValue::Px(style.line_height));
    let attr_list = AttrsList::new(attrs);
    // 主任务按时间顺序处理消息
    drop(tx); // 关闭发送端，确保任务结束后 `rx` 能正确完成
    while let Some(message) = rx.recv().await {
        match message {
            OutputLine::StdOut(line) => {
                // log::debug!("StdOut: {}",
                //             line);
                if let Ok(parsed) =
                    serde_json::from_str::<Message>(&line)
                {
                    match parsed {
                        Message::CompilerMessage(msg) => {
                            // debug!("Compiler Message: {:?}", msg);
                            if let Some(rendered) = msg.message.rendered {
                                channel.send(StyledText(parse_byte(rendered.as_bytes())))
                            }
                            // todo
                            break;
                            // log::debug!("Compiler Message: {}",
                            // msg);
                            // resolve_compiler_message(
                            //     &msg,
                            //     &style,
                            //     &mut channel
                            // );
                        }
                        Message::CompilerArtifact(_script) => {
                            // log::debug!("Compiler Artifact: {:?}",
                            // artifact);
                        }
                        Message::BuildScriptExecuted(_script) => {
                            // log::debug!("Build Script Executed:
                            // {:?}", script);
                        }
                        Message::BuildFinished(_script) => {
                            // log::debug!("Build Finished: {:?}",
                            // script);
                        }
                        Message::TextLine(_script) => {
                            // log::debug!("TextLine: {:?}", script);
                        }
                        val => {
                            log::debug!("??????????: {:?}", val);
                        }
                    }
                } else {
                    log::debug!("Non-JSON stdout: {}", line);
                }
            }
            OutputLine::StdErr(line) => {
                // let styled_text = parse_byte(line.as_bytes());
                // log::debug!("StdErr: {}",
                //             styled_text.text);
                // log::debug!("StdErr: {:?}",
                //             styled_text.styles);
                // log::debug!("stderr: {}", line);
                // channel.send(Line {
                //     content:    line,
                //     attrs_list: attr_list.clone(),
                //     hyperlink:  vec![]
                // });
            }
        }
    }

    child.wait().await?;
    Ok(())
}

pub fn create_signal_from_channel<T: Send + Clone + 'static>(
    cx: Scope
) -> (ReadSignal<Option<T>>, ExtChannel<T>, impl FnOnce(())) {
    let trigger = with_scope(cx, ExtSendTrigger::new);

    let channel_closed = cx.create_rw_signal(false);
    let (read, write) = cx.create_signal(None);
    let data = Arc::new(Mutex::new(VecDeque::new()));

    {
        let data = data.clone();
        cx.create_effect(move |_| {
            trigger.track();
            while let Some(value) = data.lock().pop_front() {
                write.set(Some(value));
            }

            if channel_closed.get() {
                cx.dispose();
            }
        });
    }

    let send = create_ext_action(cx, move |_| {
        channel_closed.set(true);
    });

    (read, ExtChannel { trigger, data }, send)
}

pub struct ExtChannel<T: Send + Clone + 'static> {
    trigger: ExtSendTrigger,
    data: Arc<Mutex<VecDeque<T>>>,
}

impl<T: Send + Clone + 'static> ExtChannel<T> {
    pub fn send(&mut self, event: T) {
        self.data.lock().push_back(event);
        register_ext_trigger(self.trigger);
    }
}

pub struct PanelStyle {
    pub font_size: f32,
    font_family: String,
    pub line_height: f32,
    pub error_color: Color,
    pub warn_color: Color,
    pub code_relative: Color,
    pub hyperlink_color: Color,
}

impl PanelStyle {
    pub fn new(
        font_size: f32,
        font_family: String,
        line_height: f32,
    ) -> Self {
        Self {
            font_size,
            font_family,
            line_height,
            error_color: Color::RED,
            warn_color: Color::YELLOW,
            code_relative: Color::rgb(116., 177., 241.),
            hyperlink_color: Color::BLUE,
        }
    }

    pub fn font_family(&self) -> Vec<FamilyOwned> {
        FamilyOwned::parse_list(&self.font_family).collect()
    }
}

#[derive(Clone)]
pub struct StyledText(pub ansi_to_style::StyledText);

impl StyledText {
    pub fn test_line_attrs(&self, range: Range<usize>) {
        self.0.styles.iter().for_each(|x| {
            if let Some(delta_range) = ranges_overlap(&x.range, &range) {
                debug!("delta_range={delta_range:?}, style: {x:?}");
            }
        });
    }
}

impl cozy_floem::data::Styled for StyledText {
    fn content(&self) -> &str {
        &self.0.text
    }

    fn line_attrs(&self, attrs_list: &mut AttrsList, default_attrs: Attrs, range: Range<usize>, delta: usize) -> Vec<Hyperlink> {
        self.0.styles.iter().filter_map(|x| {
            if let Some(delta_range) = ranges_overlap(&x.range, &range) {
                let TextStyle {
                    bold, italic, underline, fg_color, ..
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
                let range = delta_range.start - delta..delta_range.end - delta;
                debug!("delta_range={range:?}, style: {x:?}");
                attrs_list.add_span(range, attrs);
                if *underline {
                    let link = Hyperlink {
                        start_offset: delta_range.start,
                        end_offset: delta_range.end,
                        link: "".to_string(),
                        line_color: *fg_color,
                    };
                    return Some(link);
                }
            }
            None
        }).collect()
    }
}
