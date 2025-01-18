use floem::peniko::Color;
use floem::prelude::{container, Decorators, scroll, SignalGet, SignalUpdate, stack, svg, virtual_stack, VirtualDirection, VirtualItemSize};
use floem::reactive::ReadSignal;
use floem::style::{AlignItems, CursorStyle};
use floem::View;
use floem::views::static_label;
use log::error;
use crate::views::svg_from_fn;
use crate::views::tree_with_panel::data::panel::{DocManager, DocStyle};
use crate::views::tree_with_panel::data::tree::TreeNode;

pub fn view_tree(
    node: ReadSignal<TreeNode>,
    doc: DocManager,
) -> impl View {
    scroll(
        virtual_stack(
            VirtualDirection::Vertical,
            VirtualItemSize::Fixed(Box::new(move || 20.0)),
            move || node.get(),
            move |(index, _, _data)| *index,
            move |(_, level, rw_data)| {
                error!("view_tree");
                let id = rw_data.id.clone();
                let level_svg = rw_data.track_level_svg();
                let level_svg_color = rw_data.track_level_svg_color();

                let click_data = rw_data.open;
                stack(
                    (container(svg_from_fn(move || if click_data.get() {
                            r#"<svg width="16" height="16" viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="currentColor"><path fill-rule="evenodd" clip-rule="evenodd" d="M7.976 10.072l4.357-4.357.62.618L8.284 11h-.618L3 6.333l.619-.618 4.357 4.357z"/></svg>"#
                    } else {
                        r#"<svg width="16" height="16" viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="currentColor"><path fill-rule="evenodd" clip-rule="evenodd" d="M10.072 8.024L5.715 3.667l.618-.62L11 7.716v.618L6.333 13l-.618-.619 4.357-4.357z"/></svg>"#
                    }.to_string()).style(move |s| {
                        let size = 13.0;
                            s.size(size, size)
                    })).on_click_stop(move |_ | {
                        click_data.update(move |x| {
                            *x = !*x
                        });
                    }),
                    container(svg(level_svg).style(move |s| {
                        let size = 13.0;
                        if let Some(color) = level_svg_color {
                            s.size(size, size).color(color)
                        } else {
                            s.size(size, size)
                        }
                    })),
                static_label(&rw_data.content).style(move |x| x.height(23.).font_size(13.).align_self(AlignItems::Start))
                    .on_click_stop(move |_ | {
                        let value = id.clone();
                        doc.update(move |x| {
                            x.update_display(value.clone());
                        });
                    })
                )).style(move |x| x.margin_left(level as f32 * 23.0))
            },
        )
            .style(|s| s.flex_col().min_width_full().padding(6.0)),
    )
        .style(|s| s.flex_grow(1.0).size_full())
        .scroll_style(|s| s.shrink_to_fit())
        // .on_event_cont(EventListener::PointerLeave, move |_| {
        //     capture_signal_clone.highlighted.set(None)
        // })
        // .on_click_stop(move |_| capture_signal_clone.selected.set(None))
        // .scroll_to(move || {
        //     let focus_line = focus_line.get();
        //     Some((0.0, focus_line as f64 * 20.0).into())
        // })
    // .scroll_to_view(move || {
    //     let view_id = capture_signal_clone.scroll_to.get();
    //     println!("{view_id:?}");
    //     view_id
    // })
}