use cozy_floem::{data::SimpleDoc, view::panel};
use floem::{View, ViewId, keyboard::{Key, NamedKey}, prelude::{
    Decorators, RwSignal, SignalGet, SignalUpdate,
    create_rw_signal,
}, reactive::Scope, Application};
use log::{LevelFilter::Info, error};
use rust_resolve::{
    ExtChannel, create_signal_from_channel, run_command,
};
use std::thread;
use floem::kurbo::Point;
use floem::window::WindowConfig;
use tokio::process::Command;
use cozy_floem::data::StyledText;

fn main() -> anyhow::Result<()> {
    let _ = custom_utils::logger::logger_feature(
        "panel",
        "warn,rust_resolve=debug,cozy_rust_panel=debug,cozy_floem=debug",
        Info,
        false,
    )
        .build();

    let cx = Scope::new();
    let (read_signal, channel, send) =
        create_signal_from_channel::<StyledText>(cx);

    let hover_hyperlink = create_rw_signal(None);
    let doc = SimpleDoc::new(ViewId::new(), hover_hyperlink);
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

    // let style =
    //     PanelStyle::new(13.0, "JetBrains Mono".to_string(), 23.0);
    thread::spawn(|| {
        run(channel);
        send(())
    });
    // if let Err(err) = tast.join() {
    //     error!("{err:?}");
    // }
    let config = WindowConfig::default().position(Point::new(300.0, 300.));
    Application::new().window(move |_| app_view(simple_doc), Some(config)).run();
    Ok(())
}

fn app_view(simple_doc: RwSignal<SimpleDoc>) -> impl View {
    let view = panel(simple_doc).style(|x| x.width(600.).height(300.));
    let id = view.id();
    view.on_key_up(
        Key::Named(NamedKey::F11),
        |m| m.is_empty(),
        move |_| id.inspect(),
    )
}

#[tokio::main(flavor = "current_thread")]
pub async fn run(channel: ExtChannel<StyledText>) {
    if let Err(err) = _run(channel).await {
        error!("{:?}", err);
    }
}

async fn _run(channel: ExtChannel<StyledText>) -> anyhow::Result<()> {
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
        "--color=always",
        "--manifest-path",
        "D:\\git\\check_2\\Cargo.toml",
        "--package",
        "check",
        "--bin",
        "check"
    ]);
    run_command(command, channel).await?;
    Ok(())
}
