//! Layouts: inputs composed into one picture, a background plus boxes on
//! top (picture in picture, side by side, interviews).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::input::{InputId, InputRef};

/// Layouts a show can hold (the engine's compositions).
pub const LAYOUTS: usize = 4;

/// Boxes per layout.
pub const LAYOUT_BOXES: usize = 8;

/// Widest frame around a box, pixels of the program picture.
pub const BORDER_MAX_PX: u8 = 32;

/// A rectangle in fractions of the picture, from its top left corner.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BoxRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl BoxRect {
    /// Inside the picture with a size.
    pub fn is_valid(&self) -> bool {
        const SLACK: f32 = 1e-4;
        self.x >= 0.0
            && self.y >= 0.0
            && self.w > 0.0
            && self.h > 0.0
            && self.x + self.w <= 1.0 + SLACK
            && self.y + self.h <= 1.0 + SLACK
    }
}

/// One box of a layout: an input in a rectangle, with an optional white
/// frame.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LayoutBox {
    pub rect: BoxRect,
    pub input: InputId,
    /// Frame width in pixels; 0 = none.
    #[serde(default)]
    pub border_px: u8,
}

/// A layout as clients see it. Its id can stand in preview or program
/// like an input's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Layout {
    pub id: InputId,
    /// 1-based position among the layouts.
    pub number: u32,
    pub name: String,
    /// Fills the picture behind the boxes; `None` is black.
    #[serde(default)]
    pub background: Option<InputId>,
    /// Bottom to top.
    #[serde(default)]
    pub boxes: Vec<LayoutBox>,
}

/// A box as an action names it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BoxSpec {
    pub rect: BoxRect,
    pub input: InputRef,
    #[serde(default)]
    pub border_px: u8,
}
