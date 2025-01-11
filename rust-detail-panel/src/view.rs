use doc::lines::line_ending::LineEnding;
use floem::prelude::*;
use floem::reactive::{create_effect, Trigger};
use floem::taffy::NodeId;
use floem::{IntoView, Renderer, View, ViewId};
use floem::context::{PaintCx, StyleCx};
use floem::event::{Event, EventListener};
use floem::keyboard::{Key, NamedKey};
use floem::kurbo::{Line, Point, Rect, Stroke};
use floem::peniko::Color;
use floem::style::{CursorStyle, Style};
use log::{error, info};
use crate::data::{init_content, SimpleDoc};


pub fn panel(doc: RwSignal<SimpleDoc>) -> impl IntoView {
    let (hover_hyperlink, repaint) = doc.with_untracked(|x| (x.hover_hyperlink, x.repaint));
    let id = ViewId::new();
    let view = EditorView {
        id,
        inner_node: None,
        doc,
        repaint,
    }.on_event_cont(EventListener::PointerDown, move |event| {
        info!("PointerDown!!!!");
        if let Event::PointerDown(pointer_event) = event {
            let rs = doc.try_update(|x| x.pointer_down(pointer_event.clone()));
            match rs {
                Some(Err(err)) => error!("{err:?}"),
                None => error!("doc try update point down fail"),
                _ => (),
            }
        }
    }).on_event_cont(EventListener::PointerMove, move |event| {
        if let Event::PointerMove(pointer_event) = event {
            let rs = doc.try_update(|x| x.pointer_move(pointer_event.clone()));
            match rs {
                Some(Err(err)) => error!("{err:?}"),
                None => error!("doc try update point move fail"),
                _ => (),
            }
        }
    }).on_event_cont(EventListener::PointerUp, move |event| {
        if let Event::PointerUp(pointer_event) = event {
            let rs = doc.try_update(|x| x.pointer_up(pointer_event.clone()));
            match rs {
                Some(Err(err)) => error!("{err:?}"),
                None => error!("doc try update point up fail"),
                _ => (),
            }
        }
    }).keyboard_navigable().on_key_down(Key::Character("c".into()), |modifiers| modifiers.control(), move |_| {
        doc.with_untracked(|x| x.copy_select());
    }).style(move |x| {
        let hover_hyperlink = hover_hyperlink.get();
        x.apply_if(hover_hyperlink.is_some(), |x| x.cursor(CursorStyle::Pointer))
    });
    create_effect(move |_| {
        info!("repaint!!!!");
        repaint.track();
        id.request_paint();
    });
    scroll(view)
}

#[allow(dead_code)]
pub struct EditorView {
    id: ViewId,
    inner_node: Option<NodeId>,
    doc: RwSignal<SimpleDoc>,
    pub repaint: Trigger,
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
        _state: Box<dyn std::any::Any>,
    ) {}

    fn layout(
        &mut self,
        cx: &mut floem::context::LayoutCx,
    ) -> floem::taffy::prelude::NodeId {
        cx.layout_node(self.id, true, |_cx| {
            if self.inner_node.is_none() {
                self.inner_node = Some(self.id.new_taffy_node());
            }
            let (viewport_size, line_height, line_count) = self
                .doc
                .with(|x| (x.viewport.size(), x.line_height, x.visual_line.len()));
            let width = viewport_size.width + 10.0;
            let last_line_height = line_height * line_count as f64;
            let height = last_line_height.max(line_height).max(viewport_size.height);
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
        _cx: &mut floem::context::ComputeLayoutCx,
    ) -> Option<Rect> {
        None
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        self.repaint.track();
        let (line_height, lines, position_of_cursor, selections) = self
            .doc
            .with_untracked(|x| (x.line_height, x.visual_line.clone(), x.position_of_cursor(), x.select_of_cursor()));
        match selections {
            Ok(rects) => {
                for rect in rects {
                    cx.fill(&rect, &Color::parse("#C5E1C5").unwrap(), 0.0);
                }
            }
            Err(err) => {
                error!("{err:?}");
            }
        }
        match position_of_cursor {
            Ok(rect) => {
                cx.fill(&rect, &Color::BLACK, 0.0);
            }
            Err(err) => {
                error!("{err:?}");
            }
        }
        for line_info in lines {
            let y = line_info.line_index as f64 * line_height;
            let text_layout = line_info.text_layout;
            paint_extra_style(cx, &text_layout.hyperlinks);
            cx.draw_text_with_layout(text_layout.text.layout_runs(), Point::new(0.0, y));
        }
        // paint select
    }
}


pub fn paint_extra_style(
    cx: &mut PaintCx,
    extra_styles: &[(Point, Point)],
) {
    for (start, end) in extra_styles {
        cx.stroke(
            &Line::new(*start, *end),
            Color::RED,
            &Stroke::new(0.5),
        );
    }
}