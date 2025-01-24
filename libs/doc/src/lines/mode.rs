use std::fmt::Write;

use bitflags::bitflags;
use serde::{Deserialize, Serialize};


#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotionMode {
    Delete { count: usize },
    Yank { count: usize },
    Indent,
    Outdent,
}

impl MotionMode {
    pub fn count(&self) -> usize {
        match self {
            MotionMode::Delete { count } => *count,
            MotionMode::Yank { count } => *count,
            MotionMode::Indent => 1,
            MotionMode::Outdent => 1,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, Copy, Default, PartialOrd, Ord, Serialize, Deserialize)]
pub enum VisualMode {
    #[default]
    Normal,
    Linewise,
    Blockwise,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, Copy, PartialOrd, Ord)]
pub enum Mode {
    Normal,
    Insert,
    Visual(VisualMode),
    Terminal,
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Modes: u32 {
        const NORMAL = 0x1;
        const INSERT = 0x2;
        const VISUAL = 0x4;
        const TERMINAL = 0x8;
    }
}

impl From<Mode> for Modes {
    fn from(mode: Mode) -> Self {
        match mode {
            Mode::Normal => Self::NORMAL,
            Mode::Insert => Self::INSERT,
            Mode::Visual(_) => Self::VISUAL,
            Mode::Terminal => Self::TERMINAL,
        }
    }
}

impl Modes {
    pub fn parse(modes_str: &str) -> Self {
        let mut this = Self::empty();

        for c in modes_str.chars() {
            match c {
                'i' | 'I' => this.set(Self::INSERT, true),
                'n' | 'N' => this.set(Self::NORMAL, true),
                'v' | 'V' => this.set(Self::VISUAL, true),
                't' | 'T' => this.set(Self::TERMINAL, true),
                _ => {}
            }
        }

        this
    }
}

impl std::fmt::Display for Modes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bits = [
            (Self::INSERT, 'i'),
            (Self::NORMAL, 'n'),
            (Self::VISUAL, 'v'),
            (Self::TERMINAL, 't'),
        ];
        for (bit, chr) in bits {
            if self.contains(bit) {
                f.write_char(chr)?;
            }
        }

        Ok(())
    }
}
