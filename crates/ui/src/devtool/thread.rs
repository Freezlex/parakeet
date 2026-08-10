use app::MessageView;
use dioxus::prelude::*;

use super::MessageBubble;

#[component]
pub fn Thread(messages: Vec<MessageView>) -> Element {
    rsx! {
        section { class: "thread",
            if messages.is_empty() {
                p { class: "empty",
                    "No messages yet. Try sending one with the network switched off below."
                }
            }
            for message in messages {
                MessageBubble { key: "{message.id}", message }
            }
        }
    }
}
