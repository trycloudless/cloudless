use crate::components::common::tauri_invoke;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateRemoteStorageResponse {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize)]
struct AddSftpStorageRequest {
    pub storage_name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: String,
    pub password: Option<String>,
    pub private_key_pem: Option<String>,
    pub private_key_passphrase: Option<String>,
    pub remote_root_path: String,
    pub known_host_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct TestSftpConnectionRequest {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: String,
    pub password: Option<String>,
    pub private_key_pem: Option<String>,
    pub private_key_passphrase: Option<String>,
    pub remote_root_path: String,
    pub known_host_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TestSftpConnectionResponse {
    pub host_key_fingerprint: String,
    pub root_exists: bool,
    pub root_writable: bool,
}

pub fn map_sftp_storage_error(err: &str) -> String {
    let lower = err.to_lowercase();
    if lower.contains("authentication failed")
        || lower.contains("permission denied")
        || lower.contains("unauthorized")
    {
        "Authentication failed - check the username and password or private key.".to_string()
    } else if lower.contains("timed out") || lower.contains("timeout") {
        "Connection timed out - check the server address, port, firewall, and network connection."
            .to_string()
    } else if lower.contains("connection refused") {
        "Connection refused - check that SFTP is running on this port.".to_string()
    } else if lower.contains("not reachable")
        || lower.contains("network")
        || lower.contains("could not resolve")
    {
        "Server not reachable - check the server address, port, and network connection.".to_string()
    } else if lower.contains("server key") || lower.contains("host key") {
        "Server identity changed - the SFTP host key no longer matches the saved fingerprint. Do not continue unless you intentionally changed the server.".to_string()
    } else if lower.contains("remote folder is not writable")
        || lower.contains("not writable")
        || lower.contains("failed to open sftp file for writing")
    {
        "Remote folder is not writable - check permissions for this user.".to_string()
    } else if lower.contains("no such file")
        || lower.contains("not found")
        || lower.contains("does not exist")
    {
        "Remote folder not found - create the folder or choose a different path.".to_string()
    } else if lower.contains("private key could not be read")
        || lower.contains("key could not be read")
        || lower.contains("invalid key")
    {
        "Private key could not be read - paste a valid OpenSSH private key.".to_string()
    } else if lower.contains("passphrase") {
        "Private key requires a passphrase.".to_string()
    } else if lower.contains("unsupported") && lower.contains("key") {
        "Private key format is not supported. Use an OpenSSH private key.".to_string()
    } else if lower.contains("no space") || lower.contains("disk full") {
        "Remote server is out of space.".to_string()
    } else if lower.contains("sftp") {
        "SFTP connection failed - check the server details and try again.".to_string()
    } else {
        err.to_string()
    }
}

#[component]
pub fn SftpStorageForm(
    id_prefix: &'static str,
    submit_label: &'static str,
    loading_label: &'static str,
    #[prop(into)] on_success: Callback<CreateRemoteStorageResponse>,
) -> impl IntoView {
    let (storage_name, set_storage_name) = signal(String::new());
    let (host, set_host) = signal(String::new());
    let (port, set_port) = signal("22".to_string());
    let (username, set_username) = signal(String::new());
    let (auth_method, set_auth_method) = signal("password".to_string());
    let (password, set_password) = signal(String::new());
    let (private_key_pem, set_private_key_pem) = signal(String::new());
    let (private_key_passphrase, set_private_key_passphrase) = signal(String::new());
    let (remote_root_path, set_remote_root_path) = signal(String::new());
    let (show_password, set_show_password) = signal(false);
    let (show_passphrase, set_show_passphrase) = signal(false);
    let (testing, set_testing) = signal(false);
    let (loading, set_loading) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (pending_fingerprint, set_pending_fingerprint) = signal(Option::<String>::None);
    let (accepted_fingerprint, set_accepted_fingerprint) = signal(Option::<String>::None);
    let (verified_message, set_verified_message) = signal(Option::<String>::None);

    let input_class = "w-full bg-input border border-border rounded-xl px-4 py-2.5 text-text-primary text-sm focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all";
    let label_class = "block text-sm font-medium text-text-secondary mb-1.5";
    let helper_class = "text-xs text-text-secondary mt-1 mb-3";

    let parse_port = move || port.get_untracked().trim().parse::<u16>().ok();

    let validation_error = move || {
        let storage_name = storage_name.get();
        let host = host.get();
        let username = username.get();
        let remote_root_path = remote_root_path.get();
        let auth_method = auth_method.get();
        if storage_name.trim().is_empty() {
            Some("Storage name is required.".to_string())
        } else if host.trim().is_empty() {
            Some("Server is required.".to_string())
        } else if parse_port().is_none() {
            Some("Port must be between 1 and 65535.".to_string())
        } else if username.trim().is_empty() {
            Some("Username is required.".to_string())
        } else if auth_method == "password" && password.get().is_empty() {
            Some("Password is required for password authentication.".to_string())
        } else if auth_method == "private_key" && private_key_pem.get().trim().is_empty() {
            Some("SSH private key is required for private key authentication.".to_string())
        } else if remote_root_path.trim().is_empty() {
            Some("Remote folder is required.".to_string())
        } else if !remote_root_path.trim().starts_with('/') {
            Some("Remote folder must start with /.".to_string())
        } else if accepted_fingerprint.get().is_none() {
            Some(if pending_fingerprint.get().is_some() {
                "Confirm the server fingerprint before continuing.".to_string()
            } else {
                "Test the connection before continuing.".to_string()
            })
        } else {
            None
        }
    };

    let build_test_request = move |known_host_key: Option<String>| TestSftpConnectionRequest {
        host: host.get_untracked(),
        port: parse_port().unwrap_or(22),
        username: username.get_untracked(),
        auth_method: auth_method.get_untracked(),
        password: if password.get_untracked().is_empty() {
            None
        } else {
            Some(password.get_untracked())
        },
        private_key_pem: if private_key_pem.get_untracked().trim().is_empty() {
            None
        } else {
            Some(private_key_pem.get_untracked())
        },
        private_key_passphrase: if private_key_passphrase.get_untracked().is_empty() {
            None
        } else {
            Some(private_key_passphrase.get_untracked())
        },
        remote_root_path: remote_root_path.get_untracked(),
        known_host_key,
    };

    let reset_verification = move || {
        set_error.set(None);
        set_pending_fingerprint.set(None);
        set_accepted_fingerprint.set(None);
        set_verified_message.set(None);
    };

    let test_connection = move |_: leptos::ev::MouseEvent| {
        set_error.set(None);
        set_verified_message.set(None);
        set_pending_fingerprint.set(None);

        if let Some(message) = validation_error().filter(|message| {
            message != "Test the connection before continuing."
                && message != "Confirm the server fingerprint before continuing."
        }) {
            set_error.set(Some(message));
            return;
        }

        set_testing.set(true);
        let known_host_key = accepted_fingerprint.get_untracked();
        spawn_local(async move {
            let req = build_test_request(known_host_key.clone());
            match tauri_invoke::<_, TestSftpConnectionResponse>(
                "test_sftp_connection",
                "request",
                req,
            )
            .await
            {
                Ok(resp) => {
                    let _root_exists = resp.root_exists;
                    let _root_writable = resp.root_writable;
                    if known_host_key.is_some() {
                        let _ = set_accepted_fingerprint
                            .try_set(Some(resp.host_key_fingerprint.clone()));
                        let _ = set_verified_message.try_set(Some(
                            "Connection verified\nServer identity matched\nRemote folder writable"
                                .to_string(),
                        ));
                    } else {
                        let _ = set_pending_fingerprint
                            .try_set(Some(resp.host_key_fingerprint.clone()));
                    }
                }
                Err(e) => {
                    let _ = set_error.try_set(Some(map_sftp_storage_error(&e.to_string())));
                }
            }
            let _ = set_testing.try_set(false);
        });
    };

    let accept_host_key = move |_: leptos::ev::MouseEvent| {
        if let Some(fingerprint) = pending_fingerprint.get_untracked() {
            set_accepted_fingerprint.set(Some(fingerprint));
            set_pending_fingerprint.set(None);
            set_verified_message.set(Some(
                "Connection verified\nServer identity saved\nRemote folder writable".to_string(),
            ));
            set_error.set(None);
        }
    };

    let cancel_host_key = move |_: leptos::ev::MouseEvent| {
        set_pending_fingerprint.set(None);
        set_accepted_fingerprint.set(None);
        set_verified_message.set(None);
    };

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_error.set(None);
        if let Some(message) = validation_error() {
            set_error.set(Some(message));
            return;
        }
        set_loading.set(true);
        spawn_local(async move {
            let req = AddSftpStorageRequest {
                storage_name: storage_name.get_untracked(),
                host: host.get_untracked(),
                port: parse_port().unwrap_or(22),
                username: username.get_untracked(),
                auth_method: auth_method.get_untracked(),
                password: if password.get_untracked().is_empty() {
                    None
                } else {
                    Some(password.get_untracked())
                },
                private_key_pem: if private_key_pem.get_untracked().trim().is_empty() {
                    None
                } else {
                    Some(private_key_pem.get_untracked())
                },
                private_key_passphrase: if private_key_passphrase.get_untracked().is_empty() {
                    None
                } else {
                    Some(private_key_passphrase.get_untracked())
                },
                remote_root_path: remote_root_path.get_untracked(),
                known_host_key: accepted_fingerprint.get_untracked(),
            };

            match tauri_invoke::<_, CreateRemoteStorageResponse>("add_sftp_storage", "request", req)
                .await
            {
                Ok(resp) => on_success.run(resp),
                Err(e) => {
                    let _ = set_error.try_set(Some(map_sftp_storage_error(&e.to_string())));
                    let _ = set_loading.try_set(false);
                }
            }
        });
    };

    let storage_name_id = format!("{id_prefix}-storage-name");
    let host_id = format!("{id_prefix}-host");
    let port_id = format!("{id_prefix}-port");
    let username_id = format!("{id_prefix}-username");
    let auth_password_id = format!("{id_prefix}-auth-password");
    let auth_private_key_id = format!("{id_prefix}-auth-private-key");
    let password_id = format!("{id_prefix}-password");
    let private_key_id = format!("{id_prefix}-private-key");
    let passphrase_id = format!("{id_prefix}-private-key-passphrase");
    let root_path_id = format!("{id_prefix}-remote-root-path");

    view! {
        <form on:submit=on_submit class="space-y-4">
            <div>
                <label class=label_class for=storage_name_id.clone()>"Storage Name"</label>
                <input
                    type="text"
                    id=storage_name_id
                    class=input_class
                    placeholder="My SFTP Server"
                    prop:value=move || storage_name.get()
                    on:input=move |ev| { set_storage_name.set(event_target_value(&ev)); reset_verification(); }
                />
            </div>

            <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div>
                    <label class=label_class for=host_id.clone()>"Server"</label>
                    <input
                        type="text"
                        id=host_id
                        class=input_class
                        placeholder="sftp.example.com"
                        prop:value=move || host.get()
                        on:input=move |ev| { set_host.set(event_target_value(&ev)); reset_verification(); }
                    />
                    <p class=helper_class>"Use a hostname or IP address for a server you control."</p>
                </div>
                <div>
                    <label class=label_class for=port_id.clone()>"Port"</label>
                    <input
                        type="number"
                        min="1"
                        max="65535"
                        id=port_id
                        class=input_class
                        placeholder="22"
                        prop:value=move || port.get()
                        on:input=move |ev| { set_port.set(event_target_value(&ev)); reset_verification(); }
                    />
                    <p class=helper_class>"Most SFTP servers use port 22."</p>
                </div>
            </div>

            <div>
                <label class=label_class for=username_id.clone()>"Username"</label>
                <input
                    type="text"
                    id=username_id
                    class=input_class
                    placeholder="backup-user"
                    prop:value=move || username.get()
                    on:input=move |ev| { set_username.set(event_target_value(&ev)); reset_verification(); }
                />
                <p class=helper_class>"Use an account with read and write access to the remote folder."</p>
            </div>

            <fieldset>
                <legend class=label_class>"Authentication"</legend>
                <div class="flex gap-3">
                    <label class="inline-flex items-center gap-2 text-sm text-text-secondary">
                        <input
                            type="radio"
                            id=auth_password_id
                            name=format!("{id_prefix}-auth")
                            prop:checked=move || auth_method.get() == "password"
                            on:change=move |_| { set_auth_method.set("password".to_string()); reset_verification(); }
                        />
                        "Password"
                    </label>
                    <label class="inline-flex items-center gap-2 text-sm text-text-secondary">
                        <input
                            type="radio"
                            id=auth_private_key_id
                            name=format!("{id_prefix}-auth")
                            prop:checked=move || auth_method.get() == "private_key"
                            on:change=move |_| { set_auth_method.set("private_key".to_string()); reset_verification(); }
                        />
                        "SSH private key"
                    </label>
                </div>
            </fieldset>

            {move || if auth_method.get() == "password" {
                view! {
                    <div>
                        <label class=label_class for=password_id.clone()>"Password"</label>
                        <div class="relative">
                            <input
                                type=move || if show_password.get() { "text" } else { "password" }
                                id=password_id.clone()
                                class=input_class
                                placeholder="password"
                                prop:value=move || password.get()
                                on:input=move |ev| { set_password.set(event_target_value(&ev)); reset_verification(); }
                            />
                            <button
                                type="button"
                                class="absolute right-3 top-1/2 -translate-y-1/2 text-xs text-text-secondary hover:text-text-primary"
                                on:click=move |_| set_show_password.set(!show_password.get_untracked())
                            >
                                {move || if show_password.get() { "Hide" } else { "Show" }}
                            </button>
                        </div>
                        <p class=helper_class>"Stored encrypted on this device."</p>
                        <p class="text-xs text-text-secondary">"For better security, use a dedicated SFTP user with access limited to the backup folder."</p>
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="space-y-4">
                        <div>
                            <label class=label_class for=private_key_id.clone()>"SSH private key"</label>
                            <textarea
                                id=private_key_id.clone()
                                class=format!("{input_class} min-h-32 font-mono")
                                placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"
                                prop:value=move || private_key_pem.get()
                                on:input=move |ev| { set_private_key_pem.set(event_target_value(&ev)); reset_verification(); }
                            />
                            <p class=helper_class>"Paste an OpenSSH private key. It is stored encrypted on this device."</p>
                        </div>
                        <div>
                            <label class=label_class for=passphrase_id.clone()>"Private key passphrase"</label>
                            <div class="relative">
                                <input
                                    type=move || if show_passphrase.get() { "text" } else { "password" }
                                    id=passphrase_id.clone()
                                    class=input_class
                                    placeholder="optional passphrase"
                                    prop:value=move || private_key_passphrase.get()
                                    on:input=move |ev| { set_private_key_passphrase.set(event_target_value(&ev)); reset_verification(); }
                                />
                                <button
                                    type="button"
                                    class="absolute right-3 top-1/2 -translate-y-1/2 text-xs text-text-secondary hover:text-text-primary"
                                    on:click=move |_| set_show_passphrase.set(!show_passphrase.get_untracked())
                                >
                                    {move || if show_passphrase.get() { "Hide" } else { "Show" }}
                                </button>
                            </div>
                            <p class=helper_class>"Only required if your private key is encrypted."</p>
                        </div>
                        <p class="text-xs text-text-secondary">"Use a dedicated key for CloudLess and restrict the server account to the backup folder when possible."</p>
                    </div>
                }.into_any()
            }}

            <div>
                <label class=label_class for=root_path_id.clone()>"Remote folder"</label>
                <input
                    type="text"
                    id=root_path_id
                    class=input_class
                    placeholder="/backups/cloudless"
                    prop:value=move || remote_root_path.get()
                    on:input=move |ev| { set_remote_root_path.set(event_target_value(&ev)); reset_verification(); }
                />
                <p class=helper_class>"CloudLess will only write inside this folder."</p>
            </div>

            <div class="rounded-xl border border-warning-border bg-warning-tint px-4 py-3 text-xs text-warning">
                "SFTP storage depends on your server being online during backup and restore. CloudLess encrypts files before upload, but server availability and disk health are your responsibility."
            </div>

            <div>
                <button
                    type="button"
                    class="text-sm font-medium text-accent hover:text-accent-light transition-colors disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
                    on:click=test_connection
                    disabled=move || testing.get()
                >
                    {move || if testing.get() { "Testing..." } else { "Test Connection" }}
                </button>
            </div>

            {move || pending_fingerprint.get().map(|fingerprint| view! {
                <div class="rounded-xl border border-border bg-bg/60 px-4 py-3 space-y-3">
                    <div>
                        <p class="text-sm font-semibold text-text-primary">"Confirm server identity"</p>
                        <p class="text-xs text-text-secondary mt-1">
                            "CloudLess connected to this SFTP server:"
                        </p>
                        <p class="text-xs font-medium mt-1">{format!("{}:{}", host.get(), port.get())}</p>
                    </div>
                    <div>
                        <p class="text-xs text-text-secondary">"Server fingerprint:"</p>
                        <p class="text-xs font-mono break-all">{fingerprint}</p>
                    </div>
                    <p class="text-xs text-text-secondary">"Only continue if this fingerprint matches your server."</p>
                    <div class="flex gap-2">
                        <button type="button" class="px-3 py-2 rounded-lg bg-primary text-white text-sm font-medium" on:click=accept_host_key>
                            "Use this server"
                        </button>
                        <button type="button" class="px-3 py-2 rounded-lg border border-border text-text-secondary text-sm font-medium" on:click=cancel_host_key>
                            "Cancel"
                        </button>
                    </div>
                </div>
            })}

            {move || verified_message.get().map(|message| view! {
                <div class="space-y-1">
                    {message.lines().map(|line| view! {
                        <p class="text-xs text-accent">{line.to_string()}</p>
                    }).collect_view()}
                </div>
            })}

            {move || error.get().map(|e| view! {
                <div class="bg-error-tint border border-error-border text-error rounded-xl px-4 py-3 text-sm">{e}</div>
            })}

            <button
                type="submit"
                class="w-full btn-gradient text-white py-3 rounded-xl font-semibold text-lg shadow-lg shadow-primary-tint hover:shadow-xl hover:shadow-primary-glow hover:-translate-y-0.5 active:translate-y-0 transition-all disabled:opacity-50 disabled:cursor-not-allowed disabled:transform-none"
                disabled=move || loading.get() || validation_error().is_some()
            >
                {move || if loading.get() { loading_label } else { submit_label }}
            </button>
        </form>
    }
}
