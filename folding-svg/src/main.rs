use std::fs;
use floem::keyboard::{Key, NamedKey};
use floem::peniko::Brush;
use floem::prelude::*;
use floem::style::CursorStyle;
use floem::views::container;

fn app_view() -> impl IntoView {
    let view = v_stack((container(svg(svg_str("other")).style(
        move |s| {
            // Color::rgba8(0, 0, 0, 100)
            let size = 13.0;
            s.size(size, size).set_style_value(SvgColor, (Some(Brush::Solid(Color::rgba8(0, 0, 0, 120)))).into()).hover(|s| {
                s.cursor(CursorStyle::Pointer)
                    .set_style_value(SvgColor, (Some(Brush::Solid(Color::BLACK))).into())
            })
        },
    )), container(svg(svg_str("folded")).style(
        move |s| {
            let size = 13.0;
            s.size(size, size)
                .padding(2.0) // 无效
        },
    )), container(svg(svg_str("end")).style(
        move |s| {
            let size = 13.0;
            s.size(size, size)
        },
    )), container(svg(svg_str("other")).style(
        move |s| {
            let size = 13.0;
            s.size(size, size)
        },
    )).style(|x| {
        x.justify_center()
            .items_center()
    }), container(svg(svg_str("other")).style(
            move |s| {
                let size = 13.0;
                s.size(size, size)
            },
        )))).style(|x| x.margin(100.0));

    let id = view.id();
    view.on_key_up(
        Key::Named(NamedKey::F11),
        |m| m.is_empty(),
        move |_| id.inspect(),
    )
}

fn main() {
    floem::launch(app_view);
}

fn svg_str(svg_name: &str) -> String {
    match svg_name {
        "start" => {
            fs::read_to_string("resources/svg/folding-start.svg").unwrap()
        }
        "folded" => {
            fs::read_to_string("resources/svg/folding-folded.svg").unwrap()
        }
        "end" => {
            fs::read_to_string("resources/svg/folding-end.svg").unwrap()
        }
        "other" => {
            fs::read_to_string("resources/svg/folding-compare.svg").unwrap()
        }
        _ => {
            panic!()
        }
    }
}
