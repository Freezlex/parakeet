mod bubble;
mod composer;
mod device_screen;
mod resolution_panel;
mod thread;
pub mod demo;

pub use bubble::MessageBubble;
pub use composer::Composer;
pub use device_screen::DeviceScreen;
pub use resolution_panel::ResolutionPanel;
pub use thread::Thread;

use std::sync::Arc;

use dioxus::prelude::*;

use demo::{DemoDevice, DemoWorld};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Alice,
    Bob,
}

impl Side {
    pub fn device(self, world: &DemoWorld) -> &DemoDevice {
        match self {
            Side::Alice => &world.alice,
            Side::Bob => &world.bob,
        }
    }

    pub fn other(self, world: &DemoWorld) -> &DemoDevice {
        match self {
            Side::Alice => &world.bob,
            Side::Bob => &world.alice,
        }
    }
}

#[derive(Clone, Copy)]
pub struct DemoState {
    pub world: Signal<Arc<DemoWorld>>,
    pub revision: Signal<u64>,

    pub auto_sync_peer: Signal<bool>,
}

impl DemoState {
    pub fn world(&self) -> Arc<DemoWorld> {
        self.world.peek().clone()
    }

    pub fn touch(&mut self) {
        *self.revision.write() += 1;
    }
}

const DEVTOOL_CSS: Asset = asset!("/assets/styling/devtool.css");

#[component]
pub fn ChatScreen() -> Element {
    let world = use_hook(|| Signal::new(Arc::new(DemoWorld::new())));
    use_context_provider(|| DemoState {
        world,
        revision: Signal::new(0),
        auto_sync_peer: Signal::new(true),
    });

    rsx! {
        document::Link { rel: "stylesheet", href: DEVTOOL_CSS }
        div { id: "parakeet",
            header { class: "app-header",
                div {
                    h1 { "Parakeet" }
                    p { class: "tagline", "matrix by default · sms when it has to be" }
                }
                ConnectionPill {}
            }
            div { class: "demo-grid",
                DeviceScreen { side: Side::Alice }
                DeviceScreen { side: Side::Bob }
                ResolutionPanel {}
            }
        }
    }
}

#[component]
fn ConnectionPill() -> Element {
    let state = use_context::<DemoState>();
    let online = use_memo(move || {
        state.revision.read();
        state.world().server_is_up()
    });

    rsx! {
        span {
            class: if online() { "pill pill-online" } else { "pill pill-offline" },
            if online() { "homeserver up" } else { "homeserver down" }
        }
    }
}

pub(crate) fn clock_time(ts_ms: u64) -> String {
    let seconds = ts_ms / 1_000;
    format!("{:02}:{:02}", (seconds / 3_600) % 24, (seconds / 60) % 60)
}
