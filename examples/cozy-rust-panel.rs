use cozy_floem::views::{
    panel::{DocManager, DocStyle, panel},
    tree_with_panel::data::StyledText
};
use floem::{
    Application, View, ViewId,
    keyboard::{Key, NamedKey},
    kurbo::Point,
    prelude::{Decorators, SignalGet, create_rw_signal},
    reactive::Scope,
    views::stack,
    window::WindowConfig
};
use log::{LevelFilter::Info, error};
use rust_resolve::{
    ExtChannel, create_signal_from_channel, run_command
};
use std::thread;
use tokio::process::Command;

fn main() -> anyhow::Result<()> {
    let _ = custom_utils::logger::logger_feature(
        "panel",
        "warn,rust_resolve=debug,cozy_rust_panel=debug,\
         cozy_floem=debug",
        Info,
        false
    )
    .build();

    let cx = Scope::new();
    let (read_signal, channel, send) =
        create_signal_from_channel::<StyledText>(cx);

    let hover_hyperlink = create_rw_signal(None);

    let simple_doc = DocManager::new(
        cx,
        ViewId::new(),
        hover_hyperlink,
        DocStyle::default()
    );

    cx.create_effect(move |_| {
        if let Some(line) = read_signal.get() {
            simple_doc.update(|x| {
                if let Err(err) = x.append_lines(line) {
                    error!("{err:?}");
                }
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
    let config =
        WindowConfig::default().position(Point::new(300.0, 300.));
    Application::new()
        .window(move |_| app_view(simple_doc), Some(config))
        .run();
    Ok(())
}

fn app_view(simple_doc: DocManager) -> impl View {
    let view = stack((
        panel(simple_doc).style(|x| x.width(600.).height(300.)),
        // static_label("click")
        //     .style(|x| x.width(50.).height(50.))
        //     .on_click_stop(move |_| {
        //         simple_doc.update(|x| {
        //             let src = match &x.lines.display_strategy {
        //                 DisplayStrategy::Viewport => {
        //                     x.lines.ropes.keys().next().cloned()
        //                 },
        //                 DisplayStrategy::TextSrc(_) => None
        //             };
        //             x.update_display(src);
        //         });
        //     })
    ));
    let id = view.id();

    view.on_key_up(
        Key::Named(NamedKey::F11),
        |m| m.is_empty(),
        move |_| id.inspect()
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
