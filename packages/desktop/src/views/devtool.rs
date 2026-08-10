use dioxus::prelude::*;
use ui::ChatScreen;

#[component]
pub fn DevTool() -> Element {
    rsx! {
        ChatScreen {}
    }
}