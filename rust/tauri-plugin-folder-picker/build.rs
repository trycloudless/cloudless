const COMMANDS: &[&str] = &["pick_folder", "restore_access"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .ios_path("ios")
        .try_build()
        .unwrap();
}
