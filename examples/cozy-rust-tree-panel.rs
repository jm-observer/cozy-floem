use cozy_floem::views::tree_with_panel::{data::StyledText, panel, view_tree};
use floem::{
    Application, keyboard::{Key, NamedKey}, kurbo::Point,
    prelude::{
        create_rw_signal, Decorators, SignalGet,
        SignalUpdate
    },
    reactive::Scope,
    View,
    ViewId,
    views::{stack, static_label},
    window::WindowConfig
};
use log::{error, LevelFilter::Info};
use rust_resolve::{
    create_signal_from_channel, ExtChannel, run_command
};
use std::thread;
use floem::reactive::ReadSignal;
use tokio::process::Command;
use cozy_floem::views::tree_with_panel::data::lines::DisplayStrategy;
use cozy_floem::views::tree_with_panel::data::panel::{DocManager};
use cozy_floem::views::tree_with_panel::data::tree::TreeNode;

fn main() -> anyhow::Result<()> {
    let _ = custom_utils::logger::logger_feature(
        "panel",
        "warn,rust_resolve=debug,cozy_rust_panel=debug,\
         cozy_floem=debug,cozy_rust_tree_panel=debug",
        Info,
        false
    )
        .build();

    let cx = Scope::new();
    let (read_signal, channel, send) =
        create_signal_from_channel::<StyledText>(cx);

    let hover_hyperlink = create_rw_signal(None);
    let simple_doc = DocManager::new(cx, ViewId::new(), hover_hyperlink);
    let node = cx.create_rw_signal(TreeNode::Root { children: vec![], content: "Run Cargo Command".to_string() });
    let read_node = node.read_only();

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
    let config =
        WindowConfig::default().position(Point::new(300.0, 300.));
    Application::new()
        .window(move |_| app_view(read_node, simple_doc), Some(config))
        .run();
    Ok(())
}

fn app_view(node: ReadSignal<TreeNode>,
            doc: DocManager) -> impl View {
    let view = stack((view_tree(node, doc),
        panel(doc).style(|x| x.width(600.).height(300.)),
                      static_label("click")
                          .style(|x| x.width(50.).height(50.))
                          .on_click_stop(move |_| {
                              doc.update(|x| {
                                  let src = match &x.lines.display_strategy {
                                      DisplayStrategy::Viewport => {
                                          x.lines.ropes.keys().next().cloned()
                                      },
                                      DisplayStrategy::TextSrc(_) => None
                                  };
                                  x.update_display(src);
                              });
                          })
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
