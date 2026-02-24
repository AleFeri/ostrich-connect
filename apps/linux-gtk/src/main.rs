use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Button, ComboBoxText, Entry, Image, Label,
    Orientation, ScrolledWindow, TextBuffer, TextView,
};
use oc_backend::{Backend, ProtocolRegistry};
use oc_core::command::{UiCommand, UiResponse};
use oc_core::types::{ConnectionProfile, ConnectionSecurity, ProtocolKind, SessionId};
use oc_protocol_ftp::FtpProtocolFactory;
use oc_protocol_ftps::FtpsProtocolFactory;
use oc_protocol_sftp::SftpProtocolFactory;
use secrecy::SecretString;
use tokio::runtime::Runtime;

struct AppState {
    runtime: Runtime,
    backend: Backend,
    session_id: Option<SessionId>,
    default_editor: String,
    log_buffer: TextBuffer,
}

fn build_backend() -> Backend {
    let mut registry = ProtocolRegistry::default();
    registry.register(FtpProtocolFactory::new());
    registry.register(SftpProtocolFactory::new());
    registry.register(FtpsProtocolFactory::new());
    Backend::new(registry)
}

fn profile_for(protocol: ProtocolKind) -> ConnectionProfile {
    let (port, security, passive_mode) = match protocol {
        ProtocolKind::Ftp => (21, ConnectionSecurity::PlainText, true),
        ProtocolKind::Ftps => (21, ConnectionSecurity::TlsExplicit, true),
        ProtocolKind::Sftp => (22, ConnectionSecurity::SshTransport, false),
    };

    ConnectionProfile {
        protocol,
        host: "localhost".to_owned(),
        port,
        username: "demo".to_owned(),
        password: Some(SecretString::new("change-me".to_owned())),
        private_key_pem: None,
        private_key_path: None,
        security,
        strict_host_key_checking: true,
        passive_mode,
    }
}

fn protocol_from_combo_id(id: Option<glib::GString>) -> ProtocolKind {
    match id.as_deref() {
        Some("ftp") => ProtocolKind::Ftp,
        Some("ftps") => ProtocolKind::Ftps,
        _ => ProtocolKind::Sftp,
    }
}

fn append_log(buffer: &TextBuffer, line: &str) {
    let mut end = buffer.end_iter();
    buffer.insert(&mut end, &(line.to_owned() + "\n"));
}

fn dispatch(state: &Rc<RefCell<AppState>>, command: UiCommand) {
    let mut state_ref = state.borrow_mut();
    let response = state_ref
        .runtime
        .block_on(state_ref.backend.execute(command));

    match &response {
        UiResponse::Connected { session_id, .. } => state_ref.session_id = Some(*session_id),
        UiResponse::Disconnected { .. } => state_ref.session_id = None,
        UiResponse::Config { config } => {
            state_ref.default_editor = config.default_editor.clone();
        }
        _ => {}
    }

    append_log(&state_ref.log_buffer, &format!("{response:?}"));
}

fn build_ui(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("ostrich-connect (Linux GTK)")
        .default_width(920)
        .default_height(560)
        .build();

    let root = GtkBox::new(Orientation::Vertical, 8);
    let branding = GtkBox::new(Orientation::Horizontal, 8);
    let controls = GtkBox::new(Orientation::Horizontal, 8);

    if let Some(logo) = load_logo_image() {
        branding.append(&logo);
    }
    let title = Label::new(Some("ostrich-connect"));
    title.add_css_class("title-3");
    branding.append(&title);

    let selector = ComboBoxText::new();
    selector.append(Some("ftp"), "FTP");
    selector.append(Some("sftp"), "SFTP");
    selector.append(Some("ftps"), "FTPS");
    selector.set_active_id(Some("sftp"));

    let connect_btn = Button::with_label("Connect");
    let list_btn = Button::with_label("List /");
    let disconnect_btn = Button::with_label("Disconnect");
    let remote_path_entry = Entry::new();
    remote_path_entry.set_hexpand(true);
    remote_path_entry.set_placeholder_text(Some("/remote/path/file.txt"));
    let edit_btn = Button::with_label("Edit Remote");

    controls.append(&selector);
    controls.append(&connect_btn);
    controls.append(&list_btn);
    controls.append(&disconnect_btn);
    controls.append(&remote_path_entry);
    controls.append(&edit_btn);

    let log_view = TextView::new();
    log_view.set_editable(false);
    log_view.set_monospace(true);
    let log_buffer = TextBuffer::new(None);
    log_view.set_buffer(Some(&log_buffer));

    let scroll = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&log_view)
        .build();

    root.append(&branding);
    root.append(&controls);
    root.append(&scroll);
    window.set_child(Some(&root));

    let state = Rc::new(RefCell::new(AppState {
        runtime: Runtime::new().expect("tokio runtime"),
        backend: build_backend(),
        session_id: None,
        default_editor: "zed --wait".to_owned(),
        log_buffer,
    }));

    {
        let state = state.clone();
        let selector = selector.clone();
        connect_btn.connect_clicked(move |_| {
            let protocol = protocol_from_combo_id(selector.active_id());
            let profile = profile_for(protocol);
            dispatch(&state, UiCommand::Connect { profile });
        });
    }

    {
        let state = state.clone();
        list_btn.connect_clicked(move |_| {
            let session = state.borrow().session_id;
            if let Some(session_id) = session {
                dispatch(
                    &state,
                    UiCommand::ListDirectory {
                        session_id,
                        path: "/".to_owned(),
                    },
                );
            } else {
                append_log(&state.borrow().log_buffer, "No active session.");
            }
        });
    }

    {
        let state = state.clone();
        disconnect_btn.connect_clicked(move |_| {
            let session = state.borrow().session_id;
            if let Some(session_id) = session {
                dispatch(&state, UiCommand::Disconnect { session_id });
            } else {
                append_log(&state.borrow().log_buffer, "No active session.");
            }
        });
    }

    {
        let state = state.clone();
        let remote_path_entry = remote_path_entry.clone();
        edit_btn.connect_clicked(move |_| {
            let remote_path = remote_path_entry.text().to_string();
            edit_remote_file(&state, &remote_path);
        });
    }

    append_log(
        &state.borrow().log_buffer,
        "Use protocol selector + Connect. Backend contract is protocol-agnostic.",
    );
    dispatch(&state, UiCommand::LoadConfig);

    window.present();
}

fn load_logo_image() -> Option<Image> {
    const LOGO_BYTES: &[u8] = include_bytes!("../../../assets/logo.png");

    let loader = gtk4::gdk_pixbuf::PixbufLoader::new();
    loader.write(LOGO_BYTES).ok()?;
    loader.close().ok()?;
    let pixbuf = loader.pixbuf()?;
    let scaled = pixbuf
        .scale_simple(26, 26, gtk4::gdk_pixbuf::InterpType::Bilinear)
        .unwrap_or(pixbuf);
    Some(Image::from_pixbuf(Some(&scaled)))
}

fn edit_remote_file(state: &Rc<RefCell<AppState>>, remote_path: &str) {
    let remote_path = remote_path.trim();
    if remote_path.is_empty() {
        append_log(
            &state.borrow().log_buffer,
            "Enter a remote file path first.",
        );
        return;
    }

    let mut state_ref = state.borrow_mut();
    let Some(session_id) = state_ref.session_id else {
        append_log(&state_ref.log_buffer, "No active session.");
        return;
    };

    let local_path = local_edit_target_for(remote_path);
    if let Some(parent) = local_path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            append_log(
                &state_ref.log_buffer,
                &format!("Could not prepare local edit path: {error}"),
            );
            return;
        }
    }

    let local_path_str = local_path.to_string_lossy().to_string();
    let response = state_ref
        .runtime
        .block_on(state_ref.backend.execute(UiCommand::DownloadFile {
            session_id,
            remote_path: remote_path.to_owned(),
            local_path: local_path_str.clone(),
        }));
    match response {
        UiResponse::TransferCompleted { .. } => {}
        UiResponse::Error { message, .. } => {
            append_log(
                &state_ref.log_buffer,
                &format!("Could not open editor: download failed: {message}"),
            );
            let _ = std::fs::remove_file(&local_path);
            return;
        }
        other => {
            append_log(
                &state_ref.log_buffer,
                &format!("Could not open editor: unexpected response {other:?}"),
            );
            let _ = std::fs::remove_file(&local_path);
            return;
        }
    }

    let editor = if state_ref.default_editor.trim().is_empty() {
        "zed --wait".to_owned()
    } else {
        state_ref.default_editor.trim().to_owned()
    };
    append_log(
        &state_ref.log_buffer,
        &format!("Launching editor '{editor}' for {remote_path}."),
    );

    let edit_result = run_editor_with_sync(
        &state_ref.runtime,
        &mut state_ref.backend,
        session_id,
        &editor,
        remote_path,
        local_path.as_path(),
    );

    let _ = std::fs::remove_file(&local_path);

    match edit_result {
        Ok(message) => append_log(&state_ref.log_buffer, &message),
        Err(message) => append_log(&state_ref.log_buffer, &format!("Edit failed: {message}")),
    }
}

fn run_editor_with_sync(
    runtime: &Runtime,
    backend: &mut Backend,
    session_id: SessionId,
    editor: &str,
    remote_path: &str,
    local_path: &Path,
) -> Result<String, String> {
    let local_path_str = local_path.to_string_lossy().to_string();
    let command_line = format!("{editor} {}", shell_quote(&local_path_str));
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command_line)
        .spawn()
        .map_err(|error| format!("could not launch editor '{editor}': {error}"))?;

    let mut sync_count = 0usize;
    let mut last_sync_error: Option<String> = None;
    let mut last_seen_version = local_file_version(local_path).ok();
    let mut last_synced_version = last_seen_version;

    let exit_status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if let Ok(current_version) = local_file_version(local_path) {
                    if Some(current_version) != last_seen_version {
                        last_seen_version = Some(current_version);
                        match upload_edited_file(
                            runtime,
                            backend,
                            session_id,
                            local_path,
                            remote_path,
                        ) {
                            Ok(()) => {
                                last_synced_version = Some(current_version);
                                sync_count += 1;
                                last_sync_error = None;
                            }
                            Err(message) => {
                                last_sync_error = Some(message);
                            }
                        }
                    }
                }
                thread::sleep(Duration::from_millis(350));
            }
            Err(error) => {
                return Err(format!("editor process failed: {error}"));
            }
        }
    };

    if let Ok(current_version) = local_file_version(local_path) {
        if Some(current_version) != last_synced_version {
            match upload_edited_file(runtime, backend, session_id, local_path, remote_path) {
                Ok(()) => {
                    sync_count += 1;
                    last_sync_error = None;
                }
                Err(message) => {
                    last_sync_error = Some(message);
                }
            }
        }
    }

    if !exit_status.success() {
        let code = exit_status
            .code()
            .map(|value| value.to_string())
            .unwrap_or_else(|| "signal".to_owned());
        return Err(format!("editor exited with status {code}"));
    }

    if let Some(message) = last_sync_error {
        return Err(format!("edited locally but sync failed: {message}"));
    }

    if sync_count == 0 {
        Ok(format!(
            "Closed editor for {remote_path} with no local changes."
        ))
    } else if sync_count == 1 {
        Ok(format!("Saved and synced {remote_path}."))
    } else {
        Ok(format!(
            "Saved and synced {remote_path} ({sync_count} saves)."
        ))
    }
}

fn upload_edited_file(
    runtime: &Runtime,
    backend: &mut Backend,
    session_id: SessionId,
    local_path: &Path,
    remote_path: &str,
) -> Result<(), String> {
    let response = runtime.block_on(backend.execute(UiCommand::UploadFile {
        session_id,
        local_path: local_path.to_string_lossy().to_string(),
        remote_path: remote_path.to_owned(),
    }));

    match response {
        UiResponse::TransferCompleted { .. } => Ok(()),
        UiResponse::Error { message, .. } => Err(message),
        other => Err(format!("unexpected backend response: {other:?}")),
    }
}

fn local_edit_target_for(remote_path: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let file_name = Path::new(remote_path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("remote-file");

    std::env::temp_dir()
        .join("ostrich-connect-edit")
        .join(format!("{timestamp}-{file_name}"))
}

fn local_file_version(path: &Path) -> std::io::Result<(SystemTime, u64)> {
    let metadata = std::fs::metadata(path)?;
    let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
    Ok((modified, metadata.len()))
}

fn shell_quote(argument: &str) -> String {
    if argument.is_empty() {
        return "''".to_owned();
    }

    let mut quoted = String::from("'");
    for ch in argument.chars() {
        if ch == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}

fn main() {
    let app = Application::builder()
        .application_id("com.ostrich.connect.linux")
        .build();

    app.connect_activate(build_ui);
    app.run();
}
