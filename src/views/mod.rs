use floem::prelude::Svg;
use floem::reactive::create_effect;
use floem::View;

pub mod tree_with_panel;
pub mod drag_line;


pub fn svg_from_fn(svg_str: impl Fn() -> String + 'static) -> Svg {
    let content = svg_str();
    let svg = floem::views::svg(content);
    let id = svg.id();
    create_effect(move |_| {
        let new_svg_str = svg_str();
        id.update_state(new_svg_str);
    });
    svg
}
