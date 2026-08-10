use app::{MessageView, TransportBadge};
use dioxus::prelude::*;
use domain::Direction;

use super::clock_time;

#[component]
pub fn MessageBubble(message: MessageView) -> Element {
    let outgoing = message.direction == Direction::Outgoing;
    let badge = message.badge;

    rsx! {
        div { class: if outgoing { "bubble-row outgoing" } else { "bubble-row incoming" },
            div { class: "bubble",
                p { class: "bubble-body", "{message.body}" }
                div { class: "bubble-meta",
                    span { class: "time", {clock_time(message.origin_ts)} }
                    span { class: "badge badge-{badge.css_class()}", {badge.label()} }
                    span { class: "tag", title: "the id carried in the SMS trailer", "#{message.tag}" }
                }
                if let Some(reason) = message.failure {
                    p { class: "bubble-error", "{reason}" }
                }
            }
        }
    }
}

#[component]
pub fn MessageLine(message: MessageView) -> Element {
    let badge = message.badge;
    rsx! {
        li { class: "mirror-line",
            span { class: "badge badge-{badge.css_class()}", {badge.label()} }
            span { class: "mirror-body", "{message.body}" }
            span { class: "tag", "#{message.tag}" }
        }
    }
}

pub fn badge_explanation(badge: TransportBadge) -> &'static str {
    match badge {
        TransportBadge::Pending => "queued, no transport has carried it yet",
        TransportBadge::Matrix => "delivered over Matrix",
        TransportBadge::Sms => "the fallback carried it; no Matrix event yet",
        TransportBadge::Reconciled => "arrived as an SMS, then replaced by the Matrix event",
        TransportBadge::Failed => "no transport could deliver it",
    }
}
