use ansi_to_style::TextStyle;
use cozy_floem::views::tree_with_panel::{data::StyledText, panel};
use floem::{
    keyboard::{Key, NamedKey}, peniko::Color,
    prelude::{
        create_rw_signal, Decorators, RwSignal, SignalGet,
        SignalUpdate
    },
    reactive::Scope,
    text::{Attrs, AttrsList, FamilyOwned, LineHeightValue, Weight},
    View,
    ViewId
};
use log::{error, LevelFilter::Info};
use rust_resolve::{create_signal_from_channel, ExtChannel};
use std::{borrow::Cow, thread, time::Duration};
use cozy_floem::views::tree_with_panel::data::panel::SimpleDoc;

fn main() -> anyhow::Result<()> {
    let _ = custom_utils::logger::logger_feature(
        "panel",
        "error,cozy_simple_panel=debug,cozy_floem=debug",
        Info,
        false
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
            });
        }
    });

    // let style =
    //     PanelStyle::new(13.0, "JetBrains Mono".to_string(), 23.0);
    thread::spawn(|| {
        init_content(channel);
        send(())
    });
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

pub(crate) fn init_content(mut channel: ExtChannel<StyledText>) {
    let family = Cow::Owned(
        FamilyOwned::parse_list("JetBrains Mono").collect()
    );
    let font_size = 13.0;
    let attrs = Attrs::new()
        // .color(self.editor_style.ed_text_color())
        .family(&family)
        .font_size(font_size as f32)
        .line_height(LineHeightValue::Px(23.0));
    let mut attr_list = AttrsList::new(attrs);
    let attrs = Attrs::new()
        .color(Color::rgba8(214, 214, 51, 255))
        .family(&family)
        .font_size(font_size as f32)
        .weight(Weight::BOLD)
        .line_height(LineHeightValue::Px(23.0));
    attr_list.add_span(3..12, attrs);

    for i in 0..20 {
        let content = format!(
            "{}-{}",
            "   Compiling icu_collections v1.5.0         1234567890",
            i
        );
        let line = StyledText {
            id:    None,
            styled_text: ansi_to_style::StyledText {
                text:   content,
                styles: vec![TextStyle {
                    range:     3..12,
                    bold:      true,
                    italic:    false,
                    underline: false,
                    bg_color:  None,
                    fg_color:  Some(Color::rgba8(214, 214, 51, 255))
                }]
            },
            hyperlink:   vec![]
        };
        channel.send(line);
        thread::sleep(Duration::from_millis(800));
    }
}
