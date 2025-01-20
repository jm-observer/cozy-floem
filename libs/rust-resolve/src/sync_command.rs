use crate::{resolve_stderr, resolve_stdout};
use anyhow::anyhow;
use cozy_floem::{
    channel::ExtChannel, views::tree_with_panel::data::StyledText
};
use polling::{Event, PollMode, os::iocp::PollerIocpExt};
use std::{
    io::BufRead, os::windows::io::AsRawHandle, process::Command
};

pub fn run_command(
    mut command: Command,
    mut channel: ExtChannel<StyledText>
) -> anyhow::Result<()> {
    let mut child = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to start cargo build");

    let poller = polling::Poller::new()?;
    let stdout =
        child.stdout.take().ok_or(anyhow!("stdout is none"))?;
    let stderr =
        child.stderr.take().ok_or(anyhow!("stderr is none"))?;
    unsafe {
        poller.add_waitable(
            stdout.as_raw_handle(),
            Event::readable(1),
            PollMode::Oneshot
        )?;
        poller.add_waitable(
            stderr.as_raw_handle(),
            Event::readable(2),
            PollMode::Oneshot
        )?;
    }
    let mut events = polling::Events::new();
    let mut out_reader = std::io::BufReader::new(stdout).lines();
    let mut error_reader = std::io::BufReader::new(stderr).lines();

    while let Ok(n) = poller.wait(&mut events, None) {
        if n == 0 {
            break;
        }
        for event in events.iter() {
            match event.key {
                1 => {
                    while let Some(Ok(line)) = out_reader.next() {
                        if let Some(text) = resolve_stdout(&line) {
                            channel.send(text);
                        }
                    }
                },
                2 => {
                    while let Some(Ok(line)) = error_reader.next() {
                        channel.send(resolve_stderr(&line));
                    }
                },
                _ => {}
            }
        }
    }
    Ok(())
}
