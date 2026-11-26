use nsh_core::element::StyleRefinement;
use nsh_core::prelude::*;

use crate::abi::{Color, PropValue, Size as Extent};
use crate::tree::NodeState;

fn color(Color { r, g, b, a }: &Color) -> Rgba {
    rgba(u32::from_be_bytes([*r, *g, *b, *a]))
}

fn length(size: &Extent) -> Length {
    match *size {
        Extent::Px { value } => px(value).into(),
        Extent::Pct { value } => percentage(value / 100.0).into(),
        Extent::Auto | Extent::Fill => Length::Auto,
    }
}

pub(crate) fn style(mut s: StyleRefinement, state: &NodeState, is_button: bool) -> StyleRefinement {
    s = if state.word("direction") == Some("row") {
        s.flex_row()
    } else {
        s.flex_col()
    };
    if is_button {
        s = s.cursor_pointer().items_center().justify_center();
        if !state.props.contains_key("background") && !state.props.contains_key("border_width") {
            s = s
                .border_1()
                .border_color(rgb(0x888888))
                .rounded(px(4.0))
                .px(px(8.0))
                .py(px(4.0));
        }
    }
    for (key, value) in &state.props {
        s = match (key.as_str(), value) {
            ("wrap", PropValue::Flag { value: true }) => s.flex_wrap(),
            ("justify", PropValue::Text { value }) => match value.as_str() {
                "center" => s.justify_center(),
                "end" => s.justify_end(),
                "between" => s.justify_between(),
                "around" => s.justify_around(),
                _ => s.justify_start(),
            },
            ("align", PropValue::Text { value }) => match value.as_str() {
                "start" => s.items_start(),
                "center" => s.items_center(),
                "end" => s.items_end(),
                _ => s.items_stretch(),
            },
            ("width", PropValue::Extent { size }) => s.w(length(size)),
            ("height", PropValue::Extent { size }) => s.h(length(size)),
            ("min_width", PropValue::Extent { size }) => s.min_w(length(size)),
            ("max_width", PropValue::Extent { size }) => s.max_w(length(size)),
            ("min_height", PropValue::Extent { size }) => s.min_h(length(size)),
            ("max_height", PropValue::Extent { size }) => s.max_h(length(size)),
            ("gap", PropValue::Number { value }) => s.gap(px(*value)),
            ("grow", PropValue::Number { value }) => s.flex_grow(*value),
            ("shrink", PropValue::Number { value }) => s.flex_shrink(*value),
            ("padding_top", PropValue::Number { value }) => s.pt(px(*value)),
            ("padding_right", PropValue::Number { value }) => s.pr(px(*value)),
            ("padding_bottom", PropValue::Number { value }) => s.pb(px(*value)),
            ("padding_left", PropValue::Number { value }) => s.pl(px(*value)),
            ("margin_top", PropValue::Number { value }) => s.mt(px(*value)),
            ("margin_right", PropValue::Number { value }) => s.mr(px(*value)),
            ("margin_bottom", PropValue::Number { value }) => s.mb(px(*value)),
            ("margin_left", PropValue::Number { value }) => s.ml(px(*value)),
            ("border_width", PropValue::Number { value }) => s.border(px(*value)),
            ("border_color", PropValue::Paint { color: paint }) => s.border_color(color(paint)),
            ("radius", PropValue::Number { value }) => s.rounded(px(*value)),
            ("background", PropValue::Paint { color: paint }) => s.bg(color(paint)),
            ("color", PropValue::Paint { color: paint }) => s.text_color(color(paint)),
            ("opacity", PropValue::Number { value }) => s.opacity(*value),
            ("overflow", PropValue::Text { value }) if value == "hidden" => s.overflow_hidden(),
            ("cursor", PropValue::Text { value }) if value == "pointer" => s.cursor_pointer(),
            ("font_size", PropValue::Number { value }) => s.text_size(px(*value)),
            ("font_weight", PropValue::Number { value }) => {
                s.font_weight(FontWeight(*value as u16))
            }
            ("font_family", PropValue::Text { value }) => s.font_family(value.clone()),
            _ => s,
        };
    }
    if ["width", "height"].iter().any(|key| {
        matches!(
            state.props.get(*key),
            Some(PropValue::Extent { size: Extent::Fill })
        )
    }) {
        s = s.flex_1().self_stretch();
    }
    s
}
