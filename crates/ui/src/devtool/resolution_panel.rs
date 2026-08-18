use app::TransportBadge;
use dioxus::prelude::*;
use infra_sms_fake::StoredSms;

use super::bubble::{badge_explanation, MessageLine};
use super::{DemoState, Side};

const BADGES: [TransportBadge; 5] = [
    TransportBadge::Pending,
    TransportBadge::Matrix,
    TransportBadge::Sms,
    TransportBadge::Reconciled,
    TransportBadge::Failed,
];

#[component]
pub fn ResolutionPanel() -> Element {
    let state = use_context::<DemoState>();

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

    let mut auto_sync_peer = state.auto_sync_peer;

    rsx! {
        section { class: "resolution-panel",
            h2 { "State resolution" }

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
                DeviceMirror { side: Side::Alice }
                DeviceMirror { side: Side::Bob }
            }

            ActivityFeed {}
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
fn DeviceMirror(side: Side) -> Element {
    let state = use_context::<DemoState>();

    let name = use_memo(move || {
        state.revision.read();
        side.device(&state.world()).name.clone()
    });
    let messages = use_memo(move || {
        state.revision.read();
        let world = state.world();
        side.device(&world).messages(&side.other(&world).me)
    });
    let rows = use_memo(move || {
        state.revision.read();
        side.device(&state.world()).sms_storage()
    });

    rsx! {
        div { class: "card",
            h3 { "{name}'s device" }

            h4 { "Thread — as this device sees it" }
            ul { class: "mirror-list",
                if messages().is_empty() {
                    li { class: "empty", "nothing yet" }
                }
                for message in messages() {
                    MessageLine { key: "{message.id}", message }
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
        let mut entries: Vec<(String, u64, String)> = Vec::new();
        for (who, activity) in [
            ("Alice", world.alice.client.activity().recent(12)),
            ("Bob", world.bob.client.activity().recent(12)),
        ] {
            entries.extend(
                activity
                    .into_iter()
                    .map(|a| (who.to_owned(), a.ts, a.summary())),
            );
        }
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        entries
    });

    rsx! {
        div { class: "card",
            h3 { "Activity — merged" }
            ul { class: "activity-list",
                if entries().is_empty() {
                    li { class: "empty", "nothing yet" }
                }
                for (index , (who , _ts , line)) in entries().into_iter().enumerate() {
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
