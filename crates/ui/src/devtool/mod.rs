mod bubble;
mod composer;
mod conversations;
mod dev_panel;
mod thread;
pub mod demo;

pub use bubble::MessageBubble;
pub use composer::Composer;
pub use conversations::ConversationList;
pub use dev_panel::DevPanel;
pub use thread::Thread;

use std::sync::Arc;

use dioxus::prelude::*;
use domain::PeerId;

use demo::DemoWorld;

#[derive(Clone, Copy)]
pub struct DemoState {
    pub world: Signal<Arc<DemoWorld>>,
    pub revision: Signal<u64>,

    pub selected: Signal<PeerId>,

    pub auto_sync_peer: Signal<bool>,
}

impl DemoState {
    pub fn world(&self) -> Arc<DemoWorld> {
        self.world.peek().clone()
    }

    pub fn peer(&self) -> PeerId {
        self.selected.peek().clone()
    }

    pub fn touch(&mut self) {
        *self.revision.write() += 1;
    }
}

#[component]
pub fn ChatScreen() -> Element {
    let world = use_hook(|| Signal::new(Arc::new(DemoWorld::new())));
    let state = use_context_provider(|| DemoState {
        world,
        revision: Signal::new(0),
        selected: Signal::new(world.peek().bob.me.clone()),
        auto_sync_peer: Signal::new(true),
    });

    let messages = use_memo(move || {
        state.revision.read();
        state.world().alice.messages(&state.selected.read().clone())
    });

    let on_send = move |text: String| {
        let world = state.world();
        let peer = state.peer();
        let auto_sync_peer = state.auto_sync_peer;
        let mut state = state;
        spawn(async move {
            world.alice.client.send(&peer, &text).await;
            if *auto_sync_peer.peek() {
                world.bob.client.sync().await;
            }
            world.alice.client.sync().await;
            state.touch();
        });
    };

    rsx! {
        div { id: "parakeet",
            header { class: "app-header",
                div {
                    h1 { "Parakeet" }
                    p { class: "tagline", "matrix by default · sms when it has to be" }
                }
                ConnectionPill {}
            }
            ConversationList {}
            Thread { messages: messages() }
            Composer { on_send }
            DevPanel {}
        }
    }
}

#[component]
fn ConnectionPill() -> Element {
    let state = use_context::<DemoState>();
    let online = use_memo(move || {
        state.revision.read();
        state.world().alice.is_online()
    });

    rsx! {
        span {
            class: if online() { "pill pill-online" } else { "pill pill-offline" },
            if online() { "online" } else { "offline" }
        }
    }
}

pub(crate) fn clock_time(ts_ms: u64) -> String {
    let seconds = ts_ms / 1_000;
    format!("{:02}:{:02}", (seconds / 3_600) % 24, (seconds / 60) % 60)
}
