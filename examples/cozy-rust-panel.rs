use cozy_floem::{
    data::{Line, SimpleDoc},
    view::panel
};
use doc::lines::line_ending::LineEnding;
use floem::{
    View, ViewId,
    keyboard::{Key, NamedKey},
    prelude::{
        Decorators, RwSignal, SignalGet, SignalUpdate,
        create_rw_signal
    },
    reactive::Scope
};
use log::{LevelFilter::Info, info, error};
use rust_resolve::{PanelStyle, create_signal_from_channel, ExtChannel, run_command, StyledText};
use std::thread;
use tokio::process::Command;
use cozy_floem::data::Styled;

fn main() -> anyhow::Result<()> {
    let _ = custom_utils::logger::logger_feature(
        "panel",
        "error,rust_resolve=debug,cozy_rust_panel=debug,cozy_floem=debug",
        Info,
        false
    )
    .build();

    let cx = Scope::new();
    let (read_signal, channel, send) =
        create_signal_from_channel::<StyledText>(cx);

    let hover_hyperlink = create_rw_signal(None);
    let doc = SimpleDoc::new(
        ViewId::new(),
        LineEnding::CrLf,
        hover_hyperlink
    );
    let simple_doc = create_rw_signal(doc);

    cx.create_effect(move |_| {
        if let Some(line) = read_signal.get() {
            simple_doc.update(|x| {
                if let Err(err) = x.append_lines(line) {
                    error!("{err:?}");
                }
                // info!("{}", x.visual_line.len());
            });
        }
    });

    let style =
        PanelStyle::new(13.0, "JetBrains Mono".to_string(), 23.0);
    thread::spawn(|| {
        run(channel, style);
        send(())
    });
    // if let Err(err) = tast.join() {
    //     error!("{err:?}");
    // }
    floem::launch(move || app_view(simple_doc));
    Ok(())
}

fn app_view(simple_doc: RwSignal<SimpleDoc>) -> impl View {
    let view = panel(simple_doc);
    let id = view.id();
    view.on_key_up(
        Key::Named(NamedKey::F11),
        |m| m.is_empty(),
        move |_| id.inspect()
    )
}

#[tokio::main(flavor = "current_thread")]
pub async fn run(channel: ExtChannel<StyledText>, style: PanelStyle) {
    if let Err(err) = _run(channel, style).await {
        error!("{:?}", err);
    }
}
async fn _run(
    channel: ExtChannel<StyledText>,
    style: PanelStyle
) -> anyhow::Result<()> {
    let mut command = Command::new("cargo");
    command.args([
        "clean",
        "--manifest-path",
        "D:\\git\\check_2\\Cargo.toml"
    ]);
    command.output().await?;

    let mut command = Command::new("cargo");
    command.args([
        "build",
        "--message-format=json-diagnostic-rendered-ansi",
        "--color=always","--manifest-path",
        "D:\\git\\check_2\\Cargo.toml","--package","check","--bin","check"
    ]);
    run_command(command, channel, style).await?;
    Ok(())
}
