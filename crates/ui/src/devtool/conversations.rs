use dioxus::prelude::*;

use super::{clock_time, DemoState};

#[component]
pub fn ConversationList() -> Element {
    let state = use_context::<DemoState>();
    let conversations = use_memo(move || {
        state.revision.read();
        state.world().alice.client.conversations()
    });
    let selected = use_memo(move || state.selected.read().clone());

    rsx! {
        section { class: "conversations",
            if conversations().is_empty() {
                div { class: "conversation-row active",
                    span { class: "avatar", "B" }
                    div { class: "conversation-text",
                        strong { "Bob" }
                        span { class: "preview", "no messages yet" }
                    }
                }
            }
            for conversation in conversations() {
                div {
                    key: "{conversation.id}",
                    class: if conversation.peer == selected() { "conversation-row active" } else { "conversation-row" },
                    onclick: {
                        let peer = conversation.peer.clone();
                        move |_| {
                            let mut state = state;
                            state.selected.set(peer.clone());
                            state.touch();
                        }
                    },
                    span { class: "avatar",
                        {conversation.display_name.chars().next().unwrap_or('?').to_string()}
                    }
                    div { class: "conversation-text",
                        strong { "{conversation.display_name}" }
                        span { class: "preview", "{conversation.last_body}" }
                    }
                    div { class: "conversation-side",
                        span { class: "time", {clock_time(conversation.last_ts)} }
                        if conversation.pending > 0 {
                            span { class: "pending-count", "{conversation.pending} pending" }
                        }
                    }
                }
            }
        }
    }
}
