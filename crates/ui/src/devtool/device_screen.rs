use dioxus::prelude::*;

use super::{Composer, DemoState, Side, Thread};

#[component]
pub fn DeviceScreen(side: Side) -> Element {
    let state = use_context::<DemoState>();

    let name = use_memo(move || {
        state.revision.read();
        side.device(&state.world()).name.clone()
    });
    let peer_name = use_memo(move || {
        state.revision.read();
        side.other(&state.world()).name.clone()
    });
    let online = use_memo(move || {
        state.revision.read();
        side.device(&state.world()).is_online()
    });
    let messages = use_memo(move || {
        state.revision.read();
        let world = state.world();
        side.device(&world).messages(&side.other(&world).me)
    });

    let toggle_online = move |_| {
        let world = state.world();
        let auto = state.auto_sync_peer;
        let mut state = state;
        let target = !side.device(&world).is_online();
        spawn(async move {
            side.device(&world).set_online(target);
            // Coming back online is what triggers the backfill, so sync straight away.
            side.device(&world).client.sync().await;
            if *auto.peek() {
                side.other(&world).client.sync().await;
            }
            state.touch();
        });
    };

    let on_send = move |text: String| {
        let world = state.world();
        let auto = state.auto_sync_peer;
        let mut state = state;
        spawn(async move {
            let peer = side.other(&world).me.clone();
            side.device(&world).client.send(&peer, &text).await;
            if *auto.peek() {
                side.other(&world).client.sync().await;
            }
            side.device(&world).client.sync().await;
            state.touch();
        });
    };

    rsx! {
        section { class: "device-screen",
            header { class: "device-header",
                div {
                    span { class: "avatar", {name().chars().next().unwrap_or('?').to_string()} }
                    div { class: "device-text",
                        strong { "{name}" }
                        span { class: "preview", "chatting with {peer_name}" }
                    }
                }
                button {
                    class: if online() { "toggle on" } else { "toggle off" },
                    onclick: toggle_online,
                    if online() { "data on" } else { "data off" }
                }
            }
            Thread { messages: messages() }
            Composer { side, on_send }
        }
    }
}
