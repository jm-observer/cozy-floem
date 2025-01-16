use ansi_to_style::{TextStyle, parse_byte};
use anyhow::Result;
use cargo_metadata::{CompilerMessage, Message};
use cozy_floem::data::{ErrLevel, Hyperlink, ranges_overlap, StyledLines, StyledText, TextSrc};
use floem::{
    ext_event::{
        ExtSendTrigger, create_ext_action, register_ext_trigger
    },
    prelude::{SignalGet, SignalUpdate},
    reactive::{ReadSignal, Scope, with_scope},
    text::{Attrs, AttrsList, Style, Weight}
};
use log::{debug, info, warn};
use parking_lot::Mutex;
use std::{collections::VecDeque, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::mpsc
};

pub enum OutputLine {
    StdOut(String),
    StdErr(String)
}

pub async fn run_command(
    mut command: Command,
    mut channel: ExtChannel<StyledText>
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

    // let font_family = style.font_family();
    // let attrs = Attrs::new()
    //     .family(&font_family)
    //     .font_size(style.font_size)
    //     .line_height(LineHeightValue::Px(style.line_height));
    // let attr_list = AttrsList::new(attrs);
    // 主任务按时间顺序处理消息
    drop(tx); // 关闭发送端，确保任务结束后 `rx` 能正确完成
    while let Some(message) = rx.recv().await {
        match message {
            OutputLine::StdOut(line) => {
                debug!("StdOut: {}", line);
                if let Ok(parsed) =
                    serde_json::from_str::<Message>(&line)
                {
                    match parsed {
                        Message::CompilerMessage(msg) => {
                            if let Some(rendered) =
                                &msg.message.rendered
                            {
                                let styled_text =
                                    parse_byte(rendered.as_bytes());
                                let package_id = msg.package_id.clone();
                                let hyperlink =
                                    resolve_hyperlink_from_message(
                                        msg,
                                        styled_text.text.as_str()
                                    );
                                channel.send(StyledText {
                                    text_src: TextSrc::StdOut { package_id },
                                    styled_text,
                                    hyperlink
                                });
                            }
                        },
                        Message::CompilerArtifact(_script) => {
                            // log::debug!("Compiler Artifact: {:?}",
                            // artifact);
                        },
                        Message::BuildScriptExecuted(_script) => {
                            // log::debug!("Build Script Executed:
                            // {:?}", script);
                        },
                        Message::BuildFinished(_script) => {
                            // log::debug!("Build Finished: {:?}",
                            // script);
                        },
                        Message::TextLine(_script) => {
                            // log::debug!("TextLine: {:?}", script);
                        },
                        val => {
                            log::debug!("??????????: {:?}", val);
                        }
                    }
                } else {
                    log::debug!("Non-JSON stdout: {}", line);
                }
            },
            OutputLine::StdErr(line) => {
                log::debug!("StdErr: {}", line);
                let styled_text = parse_byte(line.as_bytes());
                let mut level = ErrLevel::None;
                if styled_text.text.as_str().trim_start().starts_with("error") {
                    level = ErrLevel::Error;
                }
                channel.send(StyledText {
                    text_src: TextSrc::StdErr { level },
                    styled_text,
                    hyperlink: vec![]
                });
            }
        }
    }

    child.wait().await?;
    info!("child done");
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
    data:    Arc<Mutex<VecDeque<T>>>
}

impl<T: Send + Clone + 'static> ExtChannel<T> {
    pub fn send(&mut self, event: T) {
        self.data.lock().push_back(event);
        register_ext_trigger(self.trigger);
    }
}

// pub struct PanelStyle {
//     pub font_size: f32,
//     font_family: String,
//     pub line_height: f32,
//     pub error_color: Color,
//     pub warn_color: Color,
//     pub code_relative: Color,
//     pub hyperlink_color: Color,
// }
//
// impl PanelStyle {
//     pub fn new(
//         font_size: f32,
//         font_family: String,
//         line_height: f32,
//     ) -> Self {
//         Self {
//             font_size,
//             font_family,
//             line_height,
//             error_color: Color::RED,
//             warn_color: Color::YELLOW,
//             code_relative: Color::rgb(116., 177., 241.),
//             hyperlink_color: Color::BLUE,
//         }
//     }
//
//     pub fn font_family(&self) -> Vec<FamilyOwned> {
//         FamilyOwned::parse_list(&self.font_family).collect()
//     }
// }


fn resolve_hyperlink_from_message(
    msg: CompilerMessage,
    text: &str
) -> Vec<Hyperlink> {
    let mut file_hyper: Vec<Hyperlink> = msg
        .message
        .spans
        .into_iter()
        .filter_map(|x| {
            let full_info = format!(
                "{}:{}:{}",
                x.file_name, x.line_start, x.column_start
            );
            if let Some(index) = text.find(full_info.as_str()) {
                Some(Hyperlink::File {
                    range:  index..index + full_info.len(),
                    src:    x.file_name,
                    line:   x.line_start,
                    column: Some(x.column_start)
                })
            } else {
                warn!("not found: {full_info}");
                None
            }
        })
        .collect();
    // todo
    // if let Some(code_hyper) = msg.message.code.and_then(|x| {
    //     if let Some(index) = text.find(x.code.as_str()) {
    //         Some(Hyperlink::Url {
    //             range: index..index + x.code.len(),
    //             // todo
    //             url:   "".to_string()
    //         })
    //     } else {
    //         None
    //     }
    // }) {
    //     file_hyper.push(code_hyper)
    // }
    file_hyper
}
