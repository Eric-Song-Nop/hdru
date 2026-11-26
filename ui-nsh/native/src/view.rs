use std::rc::Rc;

use nsh_base::{SelectItem, SelectValue, select};
use nsh_core::element::{AnyValue, PropError, PropSpec, Scope, TsTypes, input};
use nsh_core::prelude::*;

use crate::abi::{Payload, PropValue};
use crate::style::style;
use crate::surface::{Batch, Input};
use crate::tree::{Node, Tree};

#[derive(Clone, Default)]
pub(crate) struct Mount(Option<(async_channel::Receiver<Batch>, Input)>);

impl Mount {
    pub fn new(batches: async_channel::Receiver<Batch>, events: Input) -> Self {
        Self(Some((batches, events)))
    }
}

impl PartialEq for Mount {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Some((a, _)), Some((b, _))) => a.same_channel(b),
            (None, None) => true,
            _ => false,
        }
    }
}

// The root receives Rust-owned channels from its embedder.
impl Props for Mount {
    fn spec(_: &mut TsTypes) -> Vec<PropSpec> {
        Vec::new()
    }
    fn initial() -> Self {
        Self::default()
    }
    fn set(&mut self, name: &str, _: Option<AnyValue>) -> Result<bool, PropError> {
        Err(PropError(format!("unknown prop `{name}`")))
    }
    fn set_slot(&mut self, _: &str, _: Option<Slot>) -> bool {
        false
    }
    fn set_children(&mut self, _: Children) -> bool {
        false
    }
}

pub(crate) struct NshSurface;

impl Component for NshSurface {
    type Props = Mount;
    type Event = NoEvent;

    fn view(mount: Mount, _: Scope<NoEvent>) -> impl IntoElement {
        let Some((batches, events)) = mount.0 else {
            return view().into_element();
        };
        let mut tree = Tree::new();
        let root = render(tree.root(), events);
        spawn(async move {
            while let Ok(Batch(commands, completion)) = batches.recv().await {
                completion.complete(batch(|| tree.apply(commands)));
            }
        });
        root
    }
}

fn children(node: Rc<Node>, events: Input) -> AnyElement {
    each(
        move || node.read(|state| state.children.clone()),
        |child| child.key(),
        move |child, _| render(child.get_untracked(), events.clone()),
    )
}

fn render(node: Rc<Node>, events: Input) -> AnyElement {
    let reference = node.mount();
    let focus = node.clone();
    let on_layout = move |_: &Bounds<Points>| focus.flush_focus();
    let styling = node.clone();
    let button = node.kind == "button" || node.kind == "checkbox";
    let styling = move |s| styling.read(|state| style(s, state, button));
    let label = node.clone();
    let label = move || {
        label.read(|state| {
            state
                .word("accessible_name")
                .map(|word| SharedString::from(word.to_owned()))
        })
    };
    let enabled = node.clone();
    let disabled = move || enabled.read(|state| state.flag("disabled"));

    match node.kind.as_str() {
        "fragment" => children(node, events),
        "text" => text(move || node.read(|state| SharedString::from(state.text.clone())))
            .style_with(styling)
            .into_element(),
        "button" => {
            let source = node.clone();
            let input = events.clone();
            view()
                .role(AccessibilityRole::Button)
                .disabled(disabled)
                .element_ref(&reference)
                .on_layout(on_layout)
                .style_with(styling)
                .focus_visible(|s| s.outline(px(2.0)).outline_color(rgb(0x3182ce)))
                .on_press(move || {
                    source.emit(
                        &input,
                        "click",
                        Payload::Click {
                            value: String::new(),
                        },
                    );
                })
                .child(children(node, events))
                .into_element()
        }
        "input" => {
            let value = node.clone();
            let placeholder = node.clone();
            let edited = node.clone();
            let submitted = node.clone();
            let input_events = events.clone();
            let key_events = events.clone();
            input()
                .value(move || {
                    value.read(|state| state.word("value").unwrap_or_default().to_owned())
                })
                .placeholder(move || {
                    placeholder
                        .read(|state| state.word("placeholder").unwrap_or_default().to_owned())
                })
                .readonly(disabled.clone())
                .disabled(disabled)
                .optional_label(label)
                .element_ref(&reference)
                .on_layout(on_layout)
                .style_with(styling)
                .on_change(move |value: &str| {
                    edited.user_value(
                        "value",
                        PropValue::Text {
                            value: value.into(),
                        },
                    );
                    edited.emit(
                        &input_events,
                        "input",
                        Payload::Text {
                            value: value.into(),
                        },
                    );
                })
                .on_submit(move |value: &str| {
                    submitted.emit(
                        &events,
                        "enter",
                        Payload::Submit {
                            value: value.into(),
                        },
                    );
                })
                .on_key_press(move |value: &str| {
                    node.emit(
                        &key_events,
                        "key",
                        Payload::Key {
                            value: value.into(),
                        },
                    );
                })
                .into_element()
        }
        "checkbox" => {
            let check = node.clone();
            let pressed = node.clone();
            view()
                .role(AccessibilityRole::Checkbox)
                .disabled(disabled)
                .optional_label(label)
                .element_ref(&reference)
                .on_layout(on_layout)
                .accessibility_state(move || AccessibilityState {
                    checked: Some(if check.read(|state| state.flag("checked")) {
                        nsh_core::Checked::True
                    } else {
                        nsh_core::Checked::False
                    }),
                    ..AccessibilityState::default()
                })
                .style_with(styling)
                .focus_visible(|s| s.outline(px(2.0)).outline_color(rgb(0x3182ce)))
                .on_press(move || {
                    let value = !pressed.read(|state| state.flag("checked"));
                    pressed.user_value("checked", PropValue::Flag { value });
                    pressed.emit(&events, "change", Payload::Toggle { value });
                })
                .child(text(move || {
                    if node.read(|state| state.flag("checked")) {
                        "☑"
                    } else {
                        "☐"
                    }
                }))
                .into_element()
        }
        "select" => {
            let choices = node.clone();
            let value = node.clone();
            let trigger = node.clone();
            select()
                .items(move || {
                    choices.read(|state| match state.props.get("choices") {
                        Some(PropValue::Choices { items }) => items
                            .iter()
                            .map(|item| SelectItem::new(item.key.clone(), item.label.clone()))
                            .collect::<Vec<_>>(),
                        _ => Vec::new(),
                    })
                })
                .value(move || {
                    SelectValue::Controlled(value.read(|state| {
                        state
                            .word("value")
                            .map(|word| SharedString::from(word.to_owned()))
                    }))
                })
                .disabled(disabled)
                .accessibility_label(label)
                .popup_style(|s| s.bg(rgb(0xffffff)).border_1().border_color(rgb(0x888888)))
                .item_style(|s| s.px(px(8.0)).py(px(4.0)).text_color(rgb(0x222222)))
                .item_class("focus:bg-[#e7e7e7]")
                .trigger(view().style_with(styling).child(text(move || {
                    trigger.read(|state| {
                        let selected = state.word("value");
                        match state.props.get("choices") {
                            Some(PropValue::Choices { items }) => items
                                .iter()
                                .find(|item| Some(item.key.as_str()) == selected)
                                .map(|item| SharedString::from(format!("{} ▾", item.label)))
                                .unwrap_or_else(|| "▾".into()),
                            _ => "▾".into(),
                        }
                    })
                })))
                .on_change(move |value: &Option<SharedString>| {
                    if let Some(value) = value {
                        let value = value.to_string();
                        node.user_value(
                            "value",
                            PropValue::Text {
                                value: value.clone(),
                            },
                        );
                        node.emit(&events, "select", Payload::Selected { value });
                    }
                })
                .into_element()
        }
        _ => view()
            .style_with(styling)
            .child(children(node, events))
            .into_element(),
    }
}
