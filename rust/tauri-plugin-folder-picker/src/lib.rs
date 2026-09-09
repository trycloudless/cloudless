use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

#[cfg(mobile)]
mod mobile;

mod error;

pub use error::{Error, Result};

#[cfg(mobile)]
pub use mobile::FolderPicker;

/// Initializes the plugin.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("folder-picker")
        .setup(|app, api| {
            #[cfg(mobile)]
            {
                use tauri::Manager;
                let folder_picker = mobile::init(app, api)?;
                app.manage(folder_picker);
            }
            #[cfg(not(mobile))]
            let _ = (app, api);
            Ok(())
        })
        .build()
}
