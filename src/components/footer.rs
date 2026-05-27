//! `Footer` — bottom border. `ups` is set by the composition runner when the
//! preceding content row carried `downs` markers.

use crate::layout::{RowKind, RowSpec};

use super::component::{Component, ComponentOutput, SeparatorPolicy};
use super::context::ComponentContext;

pub struct Footer;
pub static FOOTER: Footer = Footer;

impl Component for Footer {
    fn id(&self) -> &'static str {
        "footer"
    }

    fn is_visible(&self, _ctx: &ComponentContext) -> bool {
        true
    }

    fn render(&self, _ctx: &ComponentContext) -> ComponentOutput {
        ComponentOutput {
            rows: vec![RowSpec::new(RowKind::BottomBorder)],
            leading_separator: SeparatorPolicy::None,
            top_right_chip: String::new(),
            leading_left_chip: String::new(),
        }
    }
}
