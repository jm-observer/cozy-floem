use floem::{
    kurbo::Rect,
    peniko::Color,
    reactive::{ReadSignal, RwSignal, Scope, SignalUpdate, batch},
};

use crate::lines::{
    buffer::Buffer, fold::FoldingDisplayItem, screen_lines::ScreenLines
};
use crate::lines::style::EditorStyle;

#[derive(Clone)]
pub struct Signals {
    pub(crate) show_indent_guide: SignalManager<(bool, Color)>,
    pub(crate) viewport:          SignalManager<Rect>,
    pub(crate) folding_items:     SignalManager<Vec<FoldingDisplayItem>>,
    pub(crate) screen_lines:      SignalManager<ScreenLines>,
    pub(crate) buffer_rev:        SignalManager<u64>,
    pub(crate) buffer:            SignalManager<Buffer>,
    pub(crate) pristine:          SignalManager<bool>,
    // start from 1, (line num, paint width)
    pub(crate) last_line:         SignalManager<(usize, f64)>
}

impl Signals {
    pub fn new(
        cx: Scope,
        style: &EditorStyle,
        viewport: Rect,
        buffer: Buffer,
        screen_lines: ScreenLines,
        last_line: (usize, f64)
    ) -> Self {
        let show_indent_guide = SignalManager::new(
            cx,
            (style.show_indent_guide(), style.indent_guide())
        );
        let screen_lines_signal = SignalManager::new(cx, screen_lines.clone());
        let viewport = SignalManager::new(cx, viewport);
        let folding_items_signal = SignalManager::new(cx, Vec::new());
        let rev = buffer.rev();
        let pristine = buffer.is_pristine();
        let buffer_rev = SignalManager::new(cx, rev);
        let buffer = SignalManager::new(cx, buffer);
        let last_line = SignalManager::new(cx, last_line);
        let pristine = SignalManager::new(cx, pristine);
        Self {
            show_indent_guide,
            viewport,
            folding_items: folding_items_signal,
            screen_lines: screen_lines_signal,
            buffer_rev,
            buffer,
            last_line,
            pristine
        }
    }

    // pub fn update_buffer(&mut self, buffer: Buffer) {
    //     self.buffer_rev.update_if_not_equal(buffer.rev());
    //     self.buffer.update_force(buffer);
    // }

    pub fn signal_buffer_rev(&self) -> ReadSignal<u64> {
        self.buffer_rev.signal()
    }

    pub fn trigger(&mut self) {
        batch(|| {
            self.show_indent_guide.trigger();
            self.viewport.trigger();
            self.folding_items.trigger();
            self.screen_lines.trigger();
            self.buffer_rev.trigger();
            self.buffer.trigger();
            self.last_line.trigger();
            self.pristine.trigger();
        });
    }

    pub fn trigger_force(&mut self) {
        batch(|| {
            self.show_indent_guide.trigger_force();
            self.viewport.trigger_force();
            self.folding_items.trigger_force();
            self.screen_lines.trigger_force();
            self.buffer_rev.trigger_force();
            self.buffer.trigger_force();
            self.last_line.trigger_force();
        });
    }
}

#[derive(Clone)]
pub struct SignalManager<V: Clone + 'static> {
    v:      V,
    signal: RwSignal<V>,
    dirty:  bool
}

impl<V: Clone + 'static> SignalManager<V> {
    pub fn new(cx: Scope, v: V) -> Self {
        Self {
            signal: cx.create_rw_signal(v.clone()),
            v,
            dirty: false
        }
    }

    pub fn update_force(&mut self, nv: V) {
        self.v = nv;
        self.dirty = true;
    }

    pub fn trigger(&mut self) {
        if self.dirty {
            self.signal.set(self.v.clone());
            self.dirty = false;
        }
    }

    pub fn trigger_force(&mut self) {
        self.signal.set(self.v.clone());
        self.dirty = false;
    }

    pub fn signal(&self) -> ReadSignal<V> {
        self.signal.read_only()
    }

    pub fn val(&self) -> &V {
        &self.v
    }

    pub fn val_mut(&mut self) -> &mut V {
        self.dirty = true;
        &mut self.v
    }
}

impl<V: Clone + PartialEq + 'static> SignalManager<V> {
    pub fn update_if_not_equal(&mut self, nv: V) -> bool {
        if self.v != nv {
            self.v = nv;
            self.dirty = true;
            true
        } else {
            false
        }
    }
}
