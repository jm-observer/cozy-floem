use floem::prelude::{Decorators, scroll, SignalGet, virtual_stack, VirtualDirection, VirtualItemSize};
use floem::reactive::ReadSignal;
use floem::View;
use floem::views::static_label;
use crate::views::tree_with_panel::data::panel::DocManager;
use crate::views::tree_with_panel::data::tree::TreeNode;

pub fn view_tree(
    node: ReadSignal<TreeNode>,
    doc: DocManager
) -> impl View {
    scroll(
        virtual_stack(
            VirtualDirection::Vertical,
            VirtualItemSize::Fixed(Box::new(move || 20.0)),
            move || node.get(),
            move |(index, _, _data)| *index,
            move |(_, level, rw_data)| {
                static_label(&rw_data.content).style(move |x| x.margin_left(level as f32 * 10.0))
                    .on_click_stop(move |_ | {
                        let id = rw_data.id.clone();
                        doc.update(move |x| {
                            x.update_display(id);
                        });
                    })
            },
        )
            .style(|s| s.flex_col().min_width_full()),
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