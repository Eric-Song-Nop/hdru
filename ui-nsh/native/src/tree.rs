use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use hd_run_wasmtime::native::{HostError, HostResult};
use nsh_core::prelude::{ElementRef, Signal, SignalEquals, element_ref, signal_with};

use crate::abi::{Command, Packet, Payload, PropValue};
use crate::surface::Input;

pub(crate) struct Node {
    pub id: u32,
    pub kind: String,
    generation: u32,
    pub state: RefCell<NodeState>,
    revision: Cell<Option<Signal<()>>>,
    element: Cell<Option<ElementRef>>,
    focus_pending: Cell<bool>,
}

#[derive(Default)]
pub(crate) struct NodeState {
    parent: Weak<Node>,
    pub children: Vec<Rc<Node>>,
    pub text: String,
    pub props: HashMap<String, PropValue>,
    listeners: Vec<(String, u32)>,
}

impl NodeState {
    pub fn word(&self, key: &str) -> Option<&str> {
        match self.props.get(key) {
            Some(PropValue::Text { value }) => Some(value),
            _ => None,
        }
    }

    pub fn flag(&self, key: &str) -> bool {
        matches!(self.props.get(key), Some(PropValue::Flag { value: true }))
    }
}

impl Node {
    fn new(id: u32, kind: String, generation: u32) -> Rc<Self> {
        Rc::new(Self {
            id,
            kind,
            generation,
            state: RefCell::default(),
            revision: Cell::new(None),
            element: Cell::new(None),
            focus_pending: Cell::new(false),
        })
    }

    // The node's NSH owner releases the signal and ref when it unmounts.
    // Reparenting can give a retained hd node a new NSH owner.
    pub fn mount(&self) -> ElementRef {
        self.revision
            .set(Some(signal_with((), SignalEquals::Never)));
        let element = element_ref();
        self.element.set(Some(element));
        element
    }

    pub fn read<T>(&self, read: impl FnOnce(&NodeState) -> T) -> T {
        if let Some(revision) = self.revision.get() {
            revision.try_get();
        }
        read(&self.state.borrow())
    }

    fn update(&self, edit: impl FnOnce(&mut NodeState)) {
        edit(&mut self.state.borrow_mut());
        if let Some(revision) = self.revision.get() {
            revision.try_set(());
        }
    }

    pub fn user_value(&self, key: &str, value: PropValue) {
        self.update(|state| {
            state.props.insert(key.into(), value);
        });
    }

    fn focus(&self) {
        self.focus_pending.set(true);
        self.flush_focus();
    }

    pub fn flush_focus(&self) {
        if self.focus_pending.get()
            && let Some(element) = self.element.get()
            && element.node().is_some()
        {
            self.focus_pending.set(false);
            element.focus();
        }
    }

    pub fn emit(&self, input: &Input, event: &str, payload: Payload) {
        let state = self.state.borrow();
        if state.flag("disabled") {
            return;
        }
        self.bubble(&state, input, event, &payload);
    }

    fn bubble(&self, state: &NodeState, input: &Input, event: &str, payload: &Payload) {
        for (name, handler) in &state.listeners {
            if name == event {
                input.send(Packet {
                    generation: self.generation,
                    handler: *handler,
                    payload: payload.clone(),
                });
            }
        }
        if let Some(parent) = state.parent.upgrade() {
            parent.bubble(&parent.state.borrow(), input, event, payload);
        }
    }
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Node {
    pub fn key(&self) -> (u32, u32) {
        (self.generation, self.id)
    }
}

pub(crate) struct Tree {
    nodes: HashMap<u32, Rc<Node>>,
    generation: u32,
}

impl Tree {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::from([(0, Node::new(0, "box".into(), 0))]),
            generation: 0,
        }
    }

    pub fn root(&self) -> Rc<Node> {
        self.nodes[&0].clone()
    }

    fn get(&self, id: u32) -> HostResult<Rc<Node>> {
        self.nodes
            .get(&id)
            .cloned()
            .ok_or_else(|| HostError(format!("ui: unknown node {id}")))
    }

    fn detach(node: &Rc<Node>) {
        let parent = node.state.borrow_mut().parent.upgrade();
        if let Some(parent) = parent {
            parent.update(|state| state.children.retain(|child| child.id != node.id));
        }
        node.state.borrow_mut().parent = Weak::new();
    }

    fn insert(&self, parent: u32, child: u32, before: Option<u32>) -> HostResult<()> {
        let parent = self.get(parent)?;
        let child = self.get(child)?;
        Self::detach(&child);
        child.state.borrow_mut().parent = Rc::downgrade(&parent);
        parent.update(|state| {
            let index = state
                .children
                .iter()
                .position(|node| Some(node.id) == before);
            state
                .children
                .insert(index.unwrap_or(state.children.len()), child);
        });
        Ok(())
    }

    fn forget(&mut self, first: Rc<Node>) {
        let mut removed = vec![first];
        while let Some(node) = removed.pop() {
            let mut state = node.state.borrow_mut();
            state.parent = Weak::new();
            removed.append(&mut state.children);
            self.nodes.remove(&node.id);
        }
    }

    fn clear(&mut self, node: u32) -> HostResult<()> {
        let node = self.get(node)?;
        let children = std::mem::take(&mut node.state.borrow_mut().children);
        node.update(|_| {});
        for child in children {
            self.forget(child);
        }
        Ok(())
    }

    pub fn apply(&mut self, commands: Vec<Command>) -> HostResult<()> {
        for command in commands {
            match command {
                Command::Begin { generation, .. } => {
                    self.clear(0)?;
                    self.nodes.retain(|&id, _| id == 0);
                    self.generation = generation;
                }
                Command::Create { node, kind } => {
                    self.nodes
                        .insert(node, Node::new(node, kind, self.generation));
                }
                Command::Append { parent, child } => self.insert(parent, child, None)?,
                Command::MoveBefore {
                    parent,
                    child,
                    before,
                } => self.insert(parent, child, before)?,
                Command::Remove { node } => {
                    let target = self.get(node)?;
                    Self::detach(&target);
                    self.forget(target);
                }
                Command::Clear { node } => self.clear(node)?,
                Command::Focus { node } => self.get(node)?.focus(),
                Command::SetText { node, text } => {
                    self.get(node)?.update(|state| state.text = text)
                }
                Command::SetProp { node, key, value } => {
                    if !key.contains(':') {
                        self.get(node)?.update(|state| match value {
                            PropValue::Unset => {
                                state.props.remove(&key);
                            }
                            value => {
                                state.props.insert(key, value);
                            }
                        });
                    }
                }
                Command::Listen {
                    node,
                    event,
                    handler,
                } => {
                    if !event.contains(':') {
                        self.get(node)?
                            .state
                            .borrow_mut()
                            .listeners
                            .push((event, handler));
                    }
                }
            }
        }
        Ok(())
    }
}
