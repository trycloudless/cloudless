use leptos::prelude::*;
use leptos_ui::App;

fn main() {
    console_error_panic_hook::set_once();
    web_sys::console::log_1(&"🚀 Leptos UI starting...".into());
    mount_to_body(App);
    web_sys::console::log_1(&"✅ Leptos UI mounted!".into());
}
