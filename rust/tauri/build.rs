fn main() {
    // Load .env file and expose values as build-time env vars (for env!() macro)
    // For dev builds, launch scripts write .env.dev which takes precedence
    let env_file = if std::path::Path::new(".env.dev").exists() {
        ".env.dev"
    } else {
        ".env"
    };

    if let Ok(iter) = dotenvy::from_filename_iter(env_file) {
        for item in iter.flatten() {
            println!("cargo:rustc-env={}={}", item.0, item.1);
        }
    }

    println!("cargo:rerun-if-changed=.env");
    println!("cargo:rerun-if-changed=.env.dev");

    tauri_build::build()
}
