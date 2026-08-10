use dioxus::prelude::*;

#[component]
pub fn Home() -> Element {
    rsx! {
        div {
            class: "home",
            h1 { "Welcome to Parakeet!" }
            p { "No work on the UI side for now, trying to get basic state resolution between SMS and Matrix events." }
        }
    }
}