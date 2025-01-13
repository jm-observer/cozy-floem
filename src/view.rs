use crate::data::SimpleDoc;
use floem::{
    Renderer, View, ViewId,
    context::{PaintCx, StyleCx},
    event::{Event, EventListener},
    keyboard::Key,
    kurbo::{Line, Point, Rect, Stroke},
    peniko::Color,
    prelude::{Decorators, RwSignal, SignalUpdate, SignalWith},
    reactive::SignalGet,
    style::{CursorStyle, Style},
    taffy::NodeId,
    views::scroll
};
use log::error;

pub fn panel(doc: RwSignal<SimpleDoc>) -> impl View {
    let (hover_hyperlink, id) =
        doc.with_untracked(|x| (x.hover_hyperlink, x.id));
    let view = EditorView {
        id,
        inner_node: None,
        doc
    }
    .on_event_cont(EventListener::PointerDown, move |event| {
        if let Event::PointerDown(pointer_event) = event {
            let rs = doc.try_update(|x| {
                x.pointer_down(pointer_event.clone())
            });
            match rs {
                Some(Err(err)) => error!("{err:?}"),
                None => error!("doc try update point down fail"),
                _ => ()
            }
        }
    })
    .on_event_cont(EventListener::PointerMove, move |event| {
        if let Event::PointerMove(pointer_event) = event {
            let rs = doc.try_update(|x| {
                x.pointer_move(pointer_event.clone())
            });
            match rs {
                Some(Err(err)) => error!("{err:?}"),
                None => error!("doc try update point move fail"),
                _ => ()
            }
        }
    })
    .on_event_cont(EventListener::PointerUp, move |event| {
        if let Event::PointerUp(pointer_event) = event {
            let rs = doc
                .try_update(|x| x.pointer_up(pointer_event.clone()));
            match rs {
                Some(Err(err)) => error!("{err:?}"),
                None => error!("doc try update point up fail"),
                _ => ()
            }
        }
    })
    .keyboard_navigable()
    .on_key_down(
        Key::Character("c".into()),
        |modifiers| modifiers.control(),
        move |_| {
            doc.with_untracked(|x| x.copy_select());
        }
    )
    .style(move |x| {
        let hover_hyperlink = hover_hyperlink.get();
        x.apply_if(hover_hyperlink.is_some(), |x| {
            x.cursor(CursorStyle::Pointer)
        })
    });
    scroll(view)
        .on_scroll(move |viewport| {
            doc.update(|x| {
                x.viewport = viewport;
            });
            id.request_layout();
        })
        .style(|x| x.width(300.0).height(300.0).border(1.0))
}

#[allow(dead_code)]
pub struct EditorView {
    pub id:         ViewId,
    pub inner_node: Option<NodeId>,
    pub doc:        RwSignal<SimpleDoc>
}

impl View for EditorView {
    fn id(&self) -> ViewId {
        self.id
    }

    fn style_pass(&mut self, cx: &mut StyleCx<'_>) {
        cx.app_state_mut().request_paint(self.id());
    }

    fn update(
        &mut self,
        _cx: &mut floem::context::UpdateCx,
        _state: Box<dyn std::any::Any>
    ) {
    }

    fn layout(
        &mut self,
        cx: &mut floem::context::LayoutCx
    ) -> floem::taffy::prelude::NodeId {
        cx.layout_node(self.id, true, |_cx| {
            if self.inner_node.is_none() {
                self.inner_node = Some(self.id.new_taffy_node());
            }
            let (viewport_size, line_height, line_count) =
                self.doc.with(|x| {
                    (
                        x.viewport.size(),
                        x.line_height,
                        x.visual_line.len()
                    )
                });
            let width = viewport_size.width + 10.0;
            let last_line_height = line_height * line_count as f64;
            let height = last_line_height
                .max(line_height)
                .max(viewport_size.height);
            let inner_node = self.inner_node.unwrap();
            let style = Style::new()
                .width(width)
                .height(height)
                .to_taffy_style();
            self.id.set_taffy_style(inner_node, style);

            vec![inner_node]
        })
    }

    fn compute_layout(
        &mut self,
        _cx: &mut floem::context::ComputeLayoutCx
    ) -> Option<Rect> {
        // let viewport = cx.current_viewport();
        // self.editor.doc().lines.update(|x| {
        //     if let Err(err) = x.update_viewport_size(viewport) {
        //         error!("{err:?}");
        //     }
        // });
        None
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let (
            line_height,
            lines,
            position_of_cursor,
            selections,
            style
        ) = self.doc.with_untracked(|x| {
            (
                x.line_height,
                x.visual_line.clone(),
                x.position_of_cursor(),
                x.select_of_cursor(),
                x.style
            )
        });
        match selections {
            Ok(rects) => {
                for rect in rects {
                    cx.fill(&rect, style.selection_bg, 0.0);
                }
            },
            Err(err) => {
                error!("{err:?}");
            }
        }
        // paint cursor
        match position_of_cursor {
            Ok(Some(rect)) => {
                cx.fill(&rect, Color::BLACK, 0.0);
            },
            Err(err) => {
                error!("{err:?}");
            },
            Ok(None) => {}
        }
        for line_info in lines {
            let y = line_info.line_index as f64 * line_height;
            let text_layout = line_info.text_layout;
            paint_extra_style(cx, &text_layout.hyperlinks);
            cx.draw_text_with_layout(
                text_layout.text.layout_runs(),
                Point::new(0.0, y)
            );
        }
        // paint select
    }
}

pub fn paint_extra_style(
    cx: &mut PaintCx,
    extra_styles: &[(Point, Point, Color)]
) {
    for (start, end, color) in extra_styles {
        cx.stroke(&Line::new(*start, *end), color, &Stroke::new(0.5));
    }
}
