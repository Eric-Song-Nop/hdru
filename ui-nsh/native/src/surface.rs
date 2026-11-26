use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_channel::{Receiver, Sender};
use hd_run_wasmtime::native::{CallContext, Completion, HostError, NativeCall, pending};

use crate::abi::{Command, Packet, Payload, Surface};

pub(crate) struct Batch(pub Vec<Command>, pub Completion<()>);

#[derive(Clone, Default)]
pub(crate) struct Input(Arc<Mutex<Inbox>>);

#[derive(Default)]
struct Inbox {
    generation: u32,
    closed: bool,
    events: VecDeque<Packet>,
    waiter: Option<(Arc<()>, Completion<Packet>)>,
}

fn end(generation: u32) -> Packet {
    Packet {
        generation,
        handler: 0,
        payload: Payload::Click {
            value: String::new(),
        },
    }
}

impl Input {
    fn begin(&self, generation: u32) {
        let mut state = self.0.lock().unwrap();
        let previous = std::mem::replace(&mut state.generation, generation);
        state.events.clear();
        if let Some((_, waiter)) = state.waiter.take() {
            waiter.complete(Ok(end(previous)));
        }
    }

    pub fn send(&self, packet: Packet) {
        let mut state = self.0.lock().unwrap();
        if state.closed || packet.generation != state.generation {
            return;
        }
        if let Some((_, waiter)) = state.waiter.take() {
            waiter.complete(Ok(packet));
        } else {
            state.events.push_back(packet);
        }
    }

    pub fn close(&self) {
        let mut state = self.0.lock().unwrap();
        state.closed = true;
        state.events.clear();
        if let Some((_, waiter)) = state.waiter.take() {
            waiter.complete(Ok(end(state.generation)));
        }
    }

    fn next(&self) -> NativeCall<Packet> {
        let mut state = self.0.lock().unwrap();
        if state.closed {
            return NativeCall::Ready(Ok(end(state.generation)));
        }
        if let Some(packet) = state.events.pop_front() {
            return NativeCall::Ready(Ok(packet));
        }
        let token = Arc::new(());
        let input = self.clone();
        let waiting = token.clone();
        let (call, complete) = pending(move || {
            let mut state = input.0.lock().unwrap();
            if state
                .waiter
                .as_ref()
                .is_some_and(|(current, _)| Arc::ptr_eq(current, &waiting))
            {
                state.waiter = None;
            }
        });
        state.waiter = Some((token, complete));
        NativeCall::Pending(call)
    }
}

pub(crate) struct Provider {
    commands: Sender<Batch>,
    input: Input,
}

impl Provider {
    pub fn connect() -> (Self, Receiver<Batch>, Input) {
        let (commands, batches) = async_channel::unbounded();
        let input = Input::default();
        (
            Self {
                commands,
                input: input.clone(),
            },
            batches,
            input,
        )
    }
}

impl Surface for Provider {
    fn apply(&mut self, commands: Vec<Command>, _: CallContext<'_>) -> NativeCall<()> {
        for command in &commands {
            if let Command::Begin {
                version,
                generation,
            } = command
            {
                if *version != 2 {
                    return NativeCall::Ready(Err(HostError(format!(
                        "unsupported hdru surface protocol {version}"
                    ))));
                }
                self.input.begin(*generation);
            }
        }
        let (call, completion) = pending(|| {});
        if self.commands.try_send(Batch(commands, completion)).is_err() {
            // Window shutdown closes input and detaches the renderer.
            return NativeCall::Ready(Ok(()));
        }
        NativeCall::Pending(call)
    }

    fn next_event_v2(&mut self, _: CallContext<'_>) -> NativeCall<Packet> {
        self.input.next()
    }
}
