use app::TransportBadge;
use dioxus::prelude::*;
use infra_sms_fake::StoredSms;

use super::bubble::{badge_explanation, MessageLine};
use super::DemoState;

const DEFAULT_MESSAGE_PLACEHOLDER: &str = "Type message to send";

const BADGES: [TransportBadge; 5] = [
    TransportBadge::Pending,
    TransportBadge::Matrix,
    TransportBadge::Sms,
    TransportBadge::Reconciled,
    TransportBadge::Failed,
];

const OUTGOING_SCRIPT: [&str; 4] = [
    "Switch Alice's data off, then send a message — it leaves as an SMS, id and all.",
    "Bob's SMS storage below shows the raw text ending in the #pk: trailer.",
    "Switch Alice's data back on. The queued message is re-sent to Matrix under the same id.",
    "Bob's row disappears and his message turns 'reconciled' — still exactly one message.",
];

const INCOMING_SCRIPT: [&str; 4] = [
    "Switch Bob's data off, then send as Bob — his message reaches Alice over SMS.",
    "Alice's thread shows it as 'sms', and the row is in her SMS storage below.",
    "Switch Bob's data back on so his outbox backfills to Matrix.",
    "Alice's row is deleted and her message turns 'reconciled'. The thread never grew.",
];

#[component]
pub fn DevPanel() -> Element {
    let state = use_context::<DemoState>();
    let mut alice_draft = use_signal(String::new);
    let mut bob_draft = use_signal(String::new);

    let alice_online = use_memo(move || {
        state.revision.read();
        state.world().alice.is_online()
    });
    let bob_online = use_memo(move || {
        state.revision.read();
        state.world().bob.is_online()
    });
    let server_up = use_memo(move || {
        state.revision.read();
        state.world().server_is_up()
    });
    let outbox = use_memo(move || {
        state.revision.read();
        let world = state.world();
        world.alice.client.outbox_len() + world.bob.client.outbox_len()
    });
    let awaiting = use_memo(move || {
        state.revision.read();
        let world = state.world();
        world.alice.client.awaiting_backfill().len() + world.bob.client.awaiting_backfill().len()
    });
    let events = use_memo(move || {
        state.revision.read();
        state.world().event_count()
    });

    let toggle_alice = move |_| {
        let world = state.world();
        let auto = state.auto_sync_peer;
        let mut state = state;
        let target = !world.alice.is_online();
        spawn(async move {
            world.alice.set_online(target);
            // Coming back online is what triggers the backfill, so sync straight away.
            world.alice.client.sync().await;
            if *auto.peek() {
                world.bob.client.sync().await;
            }
            state.touch();
        });
    };

    let toggle_bob = move |_| {
        let world = state.world();
        let auto = state.auto_sync_peer;
        let mut state = state;
        let target = !world.bob.is_online();
        spawn(async move {
            world.bob.set_online(target);
            world.bob.client.sync().await;
            if *auto.peek() {
                world.alice.client.sync().await;
            }
            state.touch();
        });
    };

    let toggle_server = move |_| {
        let world = state.world();
        let mut state = state;
        let target = !world.server_is_up();
        spawn(async move {
            world.set_server_up(target);
            world.alice.client.sync().await;
            world.bob.client.sync().await;
            state.touch();
        });
    };

    let sync_alice = move |_| {
        let world = state.world();
        let mut state = state;
        spawn(async move {
            world.alice.client.sync().await;
            state.touch();
        });
    };

    let sync_bob = move |_| {
        let world = state.world();
        let mut state = state;
        spawn(async move {
            world.bob.client.sync().await;
            state.touch();
        });
    };

    let send_as_alice = move |_| {
        let typed = alice_draft().trim().to_owned();
        let text = if typed.is_empty() {
            DEFAULT_MESSAGE_PLACEHOLDER.to_owned()
        } else {
            typed
        };
        alice_draft.write().clear();

        let world = state.world();
        let auto = state.auto_sync_peer;
        let mut state = state;
        spawn(async move {
            let bob = world.bob.me.clone();
            world.alice.client.send(&bob, &text).await;
            if *auto.peek() {
                world.bob.client.sync().await;
            }
            state.selected.set(world.alice.me.clone());
            state.touch();
        });
    };

    let send_as_bob = move |_| {
        let typed = bob_draft().trim().to_owned();
        let text = if typed.is_empty() {
            DEFAULT_MESSAGE_PLACEHOLDER.to_owned()
        } else {
            typed
        };
        bob_draft.write().clear();

        let world = state.world();
        let auto = state.auto_sync_peer;
        let mut state = state;
        spawn(async move {
            let alice = world.alice.me.clone();
            world.bob.client.send(&alice, &text).await;
            if *auto.peek() {
                world.alice.client.sync().await;
            }
            // Make sure the thread the message landed in is the one on screen.
            state.selected.set(world.bob.me.clone());
            state.touch();
        });
    };

    let mut auto_sync_peer = state.auto_sync_peer;

    rsx! {
        section { class: "dev-panel",
            h2 { "Dev panel" }

            div { class: "scripts",
                div {
                    h4 { "Outgoing — Alice writes while offline" }
                    ol { class: "script",
                        for step in OUTGOING_SCRIPT {
                            li { "{step}" }
                        }
                    }
                }
                div {
                    h4 { "Incoming — Bob writes while offline" }
                    ol { class: "script",
                        for step in INCOMING_SCRIPT {
                            li { "{step}" }
                        }
                    }
                }
            }

            div { class: "control-group",
                span { class: "control-label", "Alice" }
                button {
                    class: if alice_online() { "toggle on" } else { "toggle off" },
                    onclick: toggle_alice,
                    if alice_online() { "data on" } else { "data off" }
                }
                button { class: "toggle neutral", onclick: sync_alice, "Sync" },
                form { class: "as-peer",
                    onsubmit: send_as_alice,
                    input {
                        r#type: "text",
                        value: "{alice_draft}",
                        placeholder: "{DEFAULT_MESSAGE_PLACEHOLDER}",
                        autocomplete: "off",
                        oninput: move |e| alice_draft.set(e.value()),
                    }
                    button { r#type: "submit", "Send as Alice" }
                }
            }

            div { class: "control-group",
                span { class: "control-label", "Bob" }
                button {
                    class: if bob_online() { "toggle on" } else { "toggle off" },
                    onclick: toggle_bob,
                    if bob_online() { "data on" } else { "data off" }
                }
                button { class: "toggle neutral", onclick: sync_bob, "Sync" }
                form { class: "as-peer",
                    onsubmit: send_as_bob,
                    input {
                        r#type: "text",
                        value: "{bob_draft}",
                        placeholder: "{DEFAULT_MESSAGE_PLACEHOLDER}",
                        autocomplete: "off",
                        oninput: move |e| bob_draft.set(e.value()),
                    }
                    button { r#type: "submit", "Send as Bob" }
                }
            }

            div { class: "control-group",
                span { class: "control-label", "World" }
                button {
                    class: if server_up() { "toggle on" } else { "toggle off" },
                    onclick: toggle_server,
                    if server_up() { "homeserver up" } else { "homeserver down" }
                }
                label { class: "checkbox",
                    input {
                        r#type: "checkbox",
                        checked: auto_sync_peer(),
                        onchange: move |e| auto_sync_peer.set(e.checked()),
                    }
                    "the other device syncs automatically"
                }
            }

            div { class: "stats",
                Stat { label: "Queued", value: outbox().to_string() }
                Stat { label: "Awaiting backfill", value: awaiting().to_string() }
                Stat { label: "Events on server", value: events().to_string() }
            }

            div { class: "mirrors",
                OwnDevice {}
                PeerMirror {}
                ActivityFeed {}
            }

            Legend {}
        }
    }
}

#[component]
fn Stat(label: String, value: String) -> Element {
    rsx! {
        div { class: "stat",
            span { class: "stat-value", "{value}" }
            span { class: "stat-label", "{label}" }
        }
    }
}

#[component]
fn SmsRows(rows: Vec<StoredSms>) -> Element {
    rsx! {
        ul { class: "mirror-list",
            if rows.is_empty() {
                li { class: "empty", "empty" }
            }
            for row in rows {
                li { key: "{row.row}", class: "sms-row",
                    span { class: "sms-dir", if row.outgoing { "sent" } else { "recv" } }
                    code { "{row.text}" }
                }
            }
        }
    }
}

#[component]
fn OwnDevice() -> Element {
    let state = use_context::<DemoState>();
    let rows = use_memo(move || {
        state.revision.read();
        state.world().alice.sms_storage()
    });

    rsx! {
        div { class: "card",
            h3 { "Alice's device" }
            h4 {
                "SMS storage"
                span { class: "hint", " — sent copies included, as the OS keeps them" }
            }
            SmsRows { rows: rows() }
        }
    }
}

#[component]
fn PeerMirror() -> Element {
    let state = use_context::<DemoState>();

    let peer = use_memo(move || state.selected.read().clone());
    let contact = use_memo(move || {
        state.revision.read();
        let peer = state.selected.read().clone();
        state.world().contacts().into_iter().find(|c| c.peer == peer)
    });
    let messages = use_memo(move || {
        state.revision.read();
        let world = state.world();
        let peer = state.selected.read().clone();
        (peer == world.bob.me).then(|| world.bob.messages(&world.alice.me))
    });
    let rows = use_memo(move || {
        state.revision.read();
        state.world().peer_sms_storage(&state.selected.read().clone())
    });

    let name = use_memo(move || {
        contact()
            .map(|c| c.display_name)
            .unwrap_or_else(|| peer().as_str().to_owned())
    });

    rsx! {
        div { class: "card",
            h3 { "{name}'s device" }

            if let Some(messages) = messages() {
                h4 { "Thread" }
                ul { class: "mirror-list",
                    if messages.is_empty() {
                        li { class: "empty", "nothing yet" }
                    }
                    for message in messages {
                        MessageLine { key: "{message.id}", message }
                    }
                }
            } else {
                p { class: "empty",
                    "{name} does not run Parakeet — there is no app-side thread, only SMS."
                }
            }

            h4 {
                "SMS storage"
                span { class: "hint", " — the rows a real SMS app would list" }
            }
            SmsRows { rows: rows() }
        }
    }
}

#[component]
fn ActivityFeed() -> Element {
    let state = use_context::<DemoState>();

    let entries = use_memo(move || {
        state.revision.read();
        let world = state.world();
        let mut entries: Vec<(String, String)> = Vec::new();
        for (who, activity) in [
            ("Alice", world.alice.client.activity().recent(12)),
            ("Bob", world.bob.client.activity().recent(12)),
        ] {
            entries.extend(activity.into_iter().map(|a| (who.to_owned(), a.summary())));
        }
        entries
    });

    rsx! {
        div { class: "card",
            h3 { "Activity" }
            ul { class: "activity-list",
                if entries().is_empty() {
                    li { class: "empty", "nothing yet" }
                }
                for (index , (who , line)) in entries().into_iter().enumerate() {
                    li { key: "{index}-{who}",
                        span { class: "who who-{who.to_lowercase()}", "{who}" }
                        span { "{line}" }
                    }
                }
            }
        }
    }
}

#[component]
fn Legend() -> Element {
    rsx! {
        ul { class: "legend",
            for badge in BADGES {
                li { key: "{badge.css_class()}",
                    span { class: "badge badge-{badge.css_class()}", {badge.label()} }
                    span { {badge_explanation(badge)} }
                }
            }
        }
    }
}
