use ansi_to_style::parse_byte;
use anyhow::Result;
use cargo_metadata::{
    CompilerMessage, Message, diagnostic::DiagnosticLevel
};
use cozy_floem::{
    channel::ExtChannel,
    views::{
        panel::{ErrLevel, Hyperlink, TextSrc},
        tree_with_panel::data::{Level, StyledText}
    }
};
use log::{info, warn};
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
                // log::debug!("StdOut: {}", line);
                if let Ok(parsed) =
                    serde_json::from_str::<Message>(&line)
                {
                    match parsed {
                        Message::CompilerMessage(msg) => {
                            if let Some(rendered) =
                                &msg.message.rendered
                            {
                                let level = match msg.message.level {
                                    DiagnosticLevel::Ice
                                    | DiagnosticLevel::Error => {
                                        Level::Error
                                    },
                                    DiagnosticLevel::Warning => {
                                        Level::Warn
                                    },
                                    _ => Level::None
                                };

                                let styled_text =
                                    parse_byte(rendered.as_bytes());
                                let package_id =
                                    msg.package_id.clone();
                                let hyperlink =
                                    resolve_hyperlink_from_message(
                                        &msg,
                                        styled_text.text.as_str()
                                    );
                                let file =
                                    hyperlink.iter().find_map(|x| {
                                        match x {
                                            Hyperlink::File {
                                                src,
                                                ..
                                            } => Some(src.clone()),
                                            Hyperlink::Url {
                                                ..
                                            } => None
                                        }
                                    });
                                let text_src = TextSrc::StdOut {
                                    package_id,
                                    crate_name: msg.target.name,
                                    file
                                };

                                channel.send(StyledText {
                                    id: text_src,
                                    level,
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
                // log::debug!("StdErr: {}", line);
                let styled_text = parse_byte(line.as_bytes());
                let (text_src, level) = if styled_text
                    .text
                    .as_str()
                    .trim_start()
                    .starts_with("error")
                {
                    (
                        TextSrc::StdErr {
                            level: ErrLevel::Error
                        },
                        Level::Error
                    )
                } else {
                    (
                        TextSrc::StdErr {
                            level: ErrLevel::Other
                        },
                        Level::None
                    )
                };
                channel.send(StyledText {
                    id: text_src,
                    level,
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

fn resolve_hyperlink_from_message(
    msg: &CompilerMessage,
    text: &str
) -> Vec<Hyperlink> {
    let mut file_hyper: Vec<Hyperlink> = msg
        .message
        .spans
        .iter()
        .filter_map(|x| {
            let full_info = format!(
                "{}:{}:{}",
                x.file_name, x.line_start, x.column_start
            );
            if let Some(index) = text.find(full_info.as_str()) {
                Some(Hyperlink::File {
                    range:  index..index + full_info.len(),
                    src:    x.file_name.clone(),
                    line:   x.line_start,
                    column: Some(x.column_start)
                })
            } else {
                warn!("not found: {full_info}");
                None
            }
        })
        .collect();
    if let Some(code_hyper) =
        msg.message.code.as_ref().and_then(|x| {
            text.find(x.code.as_str()).map(|index| {
                Hyperlink::Url {
                    range: index..index + x.code.len(),
                    // todo
                    url:   "".to_string()
                }
            })
        })
    {
        file_hyper.push(code_hyper)
    }
    file_hyper
}
