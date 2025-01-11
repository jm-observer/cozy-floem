use doc::lines::line_ending::LineEnding;
use floem::{
    IntoView,
    peniko::Color,
    prelude::create_rw_signal,
    text::{Attrs, AttrsList, FamilyOwned, LineHeightValue, Weight}
};
use log::LevelFilter::Info;
use readonly_panel::{
    data::{Hyperlink, SimpleDoc},
    view::panel
};
use std::borrow::Cow;

fn main() {
    let _ = custom_utils::logger::logger_feature(
        "panel",
        "warn,rust_detail_panel=debug,wgpu_hal=error",
        Info,
        false
    )
    .build();
    floem::launch(app_view);
}

fn app_view() -> impl IntoView {
    let hover_hyperlink = create_rw_signal(None);
    let mut doc = SimpleDoc::new(LineEnding::CrLf, hover_hyperlink);
    for i in 0..30 {
        init_content(&mut doc, i);
    }
    let doc = create_rw_signal(doc);
    panel(doc)
}

pub(crate) fn init_content(doc: &mut SimpleDoc, i: usize) {
    let content =
        format!("{} {}", "   Compiling icu_collections v1.5.0", i);
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
        .color(Color::GREEN)
        .family(&family)
        .font_size(font_size as f32)
        .weight(Weight::BOLD)
        .line_height(LineHeightValue::Px(23.0));
    attr_list.add_span(3..12, attrs);
    doc.append_line(&content, attr_list, vec![Hyperlink {
        start_offset: 3,
        end_offset:   12,
        link:         "abc".to_string()
    }]);
}
