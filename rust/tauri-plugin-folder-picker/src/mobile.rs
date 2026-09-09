use serde::{de::DeserializeOwned, Deserialize};
use tauri::{
    plugin::{PluginApi, PluginHandle},
    AppHandle, Runtime,
};

#[cfg(target_os = "android")]
const PLUGIN_IDENTIFIER: &str = "app.tauri.folderpicker";

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_folder_picker);

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> crate::Result<FolderPicker<R>> {
    #[cfg(target_os = "android")]
    let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, "FolderPickerPlugin")?;
    #[cfg(target_os = "ios")]
    let handle = api.register_ios_plugin(init_plugin_folder_picker)?;
    Ok(FolderPicker(handle))
}

/// Access to the folder picker APIs.
#[derive(Debug)]
pub struct FolderPicker<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Clone for FolderPicker<R> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

#[derive(Debug, Deserialize)]
pub struct PickFolderResponse {
    pub path: Option<String>,
}

impl<R: Runtime> FolderPicker<R> {
    /// Opens a native folder picker and returns the selected folder's real filesystem path.
    pub fn pick_folder(&self) -> crate::Result<Option<String>> {
        let response = self
            .0
            .run_mobile_plugin::<PickFolderResponse>("pickFolder", ())?;
        Ok(response.path)
    }

    /// Restores security-scoped access to previously bookmarked folders (iOS only, no-op on Android).
    pub fn restore_access(&self) -> crate::Result<()> {
        self.0
            .run_mobile_plugin::<serde_json::Value>("restoreAccess", ())?;
        Ok(())
    }
}
