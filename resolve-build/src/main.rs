use std::collections::VecDeque;
use std::sync::{Arc};
use std::thread;
use std::time::Duration;
use doc::lines::line_ending::LineEnding;
use floem::ext_event::{create_ext_action, ExtSendTrigger, register_ext_trigger};
use floem::{IntoView, View, ViewId};
use floem::keyboard::{Key, NamedKey};
use floem::prelude::{create_rw_signal, Decorators, RwSignal, SignalGet, SignalUpdate};
use floem::reactive::{create_effect, ReadSignal, Scope, with_scope};
use log::{error, info};
use log::LevelFilter::Info;
use parking_lot::Mutex;
use resolve_build::{create_signal_from_channel, ExtChannel, PanelStyle, run};
use tokio::process::Command;
use readonly_panel::data::{Line, SimpleDoc};
use readonly_panel::view::panel;

fn main() -> anyhow::Result<()> {
    let _ = custom_utils::logger::logger_feature(
        "panel",
        "error,resolve_build=debug",
        Info,
        false,
    )
        .build();

    let cx = Scope::new();
    let (read_signal, channel, send) = create_signal_from_channel::<Line>(cx);

    let hover_hyperlink = create_rw_signal(None);
    let mut doc = SimpleDoc::new(ViewId::new(), LineEnding::CrLf, hover_hyperlink);
    let simple_doc = create_rw_signal(doc);

    cx.create_effect(move |_| {
        if let Some(line) = read_signal.get() {
            simple_doc.update(|x| {
                x.append_line(line);
                info!("{}", x.visual_line.len());
            });
        }
    });

    let style = PanelStyle::new(13.0, "JetBrains Mono".to_string(), 23.0,
    );
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
