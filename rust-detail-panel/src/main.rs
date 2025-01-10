mod data;

use doc::lines::line_ending::LineEnding;
use floem::{IntoView, peniko, Renderer, View, ViewId};
use floem::context::{PaintCx, StyleCx};
use floem::event::{Event, EventListener};
use floem::keyboard::{Key, NamedKey};
use floem::kurbo::{BezPath, Line, Point, Rect, Stroke};
use floem::peniko::Color;
use floem::prelude::{create_rw_signal, Decorators, RwSignal, SignalUpdate, SignalWith};
use floem::reactive::{create_effect, SignalGet, Trigger};
use floem::style::{CursorStyle, Style};
use floem::taffy::NodeId;
use floem::views::scroll;
use log::{error, info};
use log::LevelFilter::Info;
use crate::data::{init_content, SimpleDoc};

fn main() {
    // custom_utils::logger::logger_stdout_debug();
    let _ = custom_utils::logger::logger_feature("panel", "warn,rust_detail_panel=debug,wgpu_hal=error", Info, false).build();
    floem::launch(app_view);
}

fn app_view() -> impl IntoView {
    let hover_hyperlink = create_rw_signal(None);
    let mut doc = SimpleDoc::new(LineEnding::CrLf, hover_hyperlink);
    for i in 0..30 {
        init_content(&mut doc, i);
    }
    let id = ViewId::new();
    let repaint = doc.repaint;
    let doc = create_rw_signal(doc);
    let view = EditorView {
        id,
        inner_node: None,
        doc,
        repaint,
    }.on_event_cont(EventListener::PointerDown, move |event| {
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
    }).style(move |x| {
        let hover_hyperlink = hover_hyperlink.get();
        x.apply_if(hover_hyperlink.is_some(), |x| x.cursor(CursorStyle::Pointer))
    });
    create_effect(move |_| {
        repaint.track();
        info!("repaint.track");
        id.request_paint();
    });
    let view = scroll(view).on_scroll(move |viewport| {
        info!("on_scroll {viewport:?}");
        doc.update(|x| {
            x.viewport = viewport;
        });
        id.request_layout();
        // })
        //     .on_event_stop(EventListener::PointerMove, move |event| {
        //         if let Event::PointerMove(pointer_event) = event {
        //             e_data.get_untracked().pointer_move(pointer_event);
        //         }
        //     })
        //     .on_event_stop(EventListener::PointerUp, move |event| {
        //         if let Event::PointerUp(pointer_event) = event {
        //             e_data.get_untracked().pointer_up(pointer_event);
        //         }
    });
    view.style(|x| x.width(300.0).height(300.0).border(1.0)).on_key_up(
        Key::Named(NamedKey::F11),
        |m| m.is_empty(),
        move |_| id.inspect(),
    )
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
        // let viewport = cx.current_viewport();
        // self.editor.doc().lines.update(|x| {
        //     if let Err(err) = x.update_viewport_size(viewport) {
        //         error!("{err:?}");
        //     }
        // });
        None
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        self.repaint.track();
        let (line_height, lines, position_of_cursor, selections) = self
            .doc
            .with_untracked(|x| (x.line_height, x.visual_line.clone(), x.position_of_cursor(), x.select_of_cursor()));
        info!("paint lines={} cursor rect = {:?}", lines.len(), position_of_cursor);
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
        // paint cursor
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

pub fn paint_wave_line(cx: &mut PaintCx, width: f64, point: Point, color: Color) {
    let radius = 2.0;
    let origin = Point::new(point.x, point.y + radius);
    let mut path = BezPath::new();
    path.move_to(origin);

    let mut x = 0.0;
    let mut direction = -1.0;
    while x < width {
        let point = origin + (x, 0.0);
        let p1 = point + (radius, -radius * direction);
        let p2 = point + (radius * 2.0, 0.0);
        path.quad_to(p1, p2);
        x += radius * 2.0;
        direction *= -1.0;
    }

    cx.stroke(&path, color, &peniko::kurbo::Stroke::new(1.));
}
