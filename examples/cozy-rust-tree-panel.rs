use cozy_floem::views::tree_with_panel::{data::StyledText, panel, view_tree};
use floem::{
    Application, keyboard::{Key, NamedKey}, kurbo::Point,
    prelude::{
        create_rw_signal, Decorators, SignalGet,
        SignalUpdate,
    },
    reactive::Scope,
    View,
    ViewId,
    views::{stack, static_label},
    window::WindowConfig,
};
use log::{error, LevelFilter::Info};
use rust_resolve::{
    create_signal_from_channel, ExtChannel, run_command,
};
use std::thread;
use floem::prelude::RwSignal;
use floem::reactive::ReadSignal;
use tokio::process::Command;
use cozy_floem::views::drag_line::x_drag_line;
use cozy_floem::views::tree_with_panel::data::lines::DisplayStrategy;
use cozy_floem::views::tree_with_panel::data::panel::{DocManager, DocStyle};
use cozy_floem::views::tree_with_panel::data::tree::{Level, TreeNode};

fn main() -> anyhow::Result<()> {
    let _ = custom_utils::logger::logger_feature(
        "panel",
        "warn,rust_resolve=debug,cozy_rust_panel=debug,\
         cozy_floem=debug,cozy_rust_tree_panel=debug",
        Info,
        false,
    )
        .build();

    let cx = Scope::new();
    let (read_signal, channel, send) =
        create_signal_from_channel::<StyledText>(cx);

    let hover_hyperlink = cx.create_rw_signal(None);
    let simple_doc = DocManager::new(cx, ViewId::new(), hover_hyperlink, DocStyle::default());
    let node = cx.create_rw_signal(TreeNode::Root { cx, children: vec![], content: "Run Cargo Command".to_string() , open: cx.create_rw_signal(true), level: cx.create_rw_signal(Level::None)});
    let read_node = node.read_only();
    let left_width = cx.create_rw_signal(200.0);

    cx.create_effect(move |_| {
        if let Some(line) = read_signal.get() {
            simple_doc.update(|x| {
                if let Some(src) = &line.text_src {
                    node.update(|x| {
                        x.add_child(src.clone(), "abc11".to_string())
                    })
                }
                if let Err(err) = x.append_lines(line) {
                    error!("{err:?}");
                }
            });
        }
    });
    thread::spawn(|| {
        run(channel);
        send(())
    });
    let config =
        WindowConfig::default().position(Point::new(300.0, 300.));
    Application::new()
        .window(move |_| app_view(read_node, simple_doc, left_width), Some(config))
        .run();
    Ok(())
}

fn app_view(node: ReadSignal<TreeNode>,
            doc: DocManager, left_width: RwSignal<f64>) -> impl View {
    let view = stack((view_tree(node, doc).style(move |x| {
        let width = left_width.get();
        x.width(width).height_full().border_left(1.).border_top(1.).border_bottom(1.).border_right(1.0)
    }), x_drag_line(left_width).style(move |s| {
        s.width(6.0).height_full().margin_left(-6.0)
    }),
                      panel(doc).style(|x| x.flex_grow(1.).height_full())
    )).style(|x| x.height(300.0).width(800.0));
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
