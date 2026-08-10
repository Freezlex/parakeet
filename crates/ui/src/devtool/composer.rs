use dioxus::prelude::*;
use domain::sms_frame::TRAILER_CHARS;

use super::DemoState;

const SMS_SEGMENT_CHARS: usize = 160;

#[component]
pub fn Composer(on_send: EventHandler<String>) -> Element {
    let state = use_context::<DemoState>();
    let mut draft = use_signal(String::new);

    let online = use_memo(move || {
        state.revision.read();
        state.world().alice.is_online()
    });
    let peer_name = use_memo(move || {
        state.revision.read();
        let peer = state.selected.read().clone();
        state
            .world()
            .contacts()
            .into_iter()
            .find(|c| c.peer == peer)
            .map(|c| c.display_name)
            .unwrap_or_else(|| peer.as_str().to_owned())
    });

    let submit = move |_| {
        let text = draft().trim().to_owned();
        if text.is_empty() {
            return;
        }
        draft.write().clear();
        on_send.call(text);
    };

    let remaining = use_memo(move || {
        (SMS_SEGMENT_CHARS as isize) - (draft().chars().count() + TRAILER_CHARS) as isize
    });

    rsx! {
        form { class: "composer",
            onsubmit: submit,
            input {
                r#type: "text",
                value: "{draft}",
                placeholder: "Message {peer_name}",
                autocomplete: "off",
                oninput: move |e| draft.set(e.value()),
            }
            button { r#type: "submit", disabled: draft().trim().is_empty(), "Send" }
        }
        if !online() {
            p { class: "composer-hint",
                "Offline — this will go out as an SMS. "
                "{remaining()} characters left in the first segment after the {TRAILER_CHARS}-character id trailer."
            }
        }
    }
}
