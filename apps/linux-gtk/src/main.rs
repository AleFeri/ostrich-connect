use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Button, ComboBoxText, Image, Label, Orientation,
    ScrolledWindow, TextBuffer, TextView,
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

    controls.append(&selector);
    controls.append(&connect_btn);
    controls.append(&list_btn);
    controls.append(&disconnect_btn);

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

fn main() {
    let app = Application::builder()
        .application_id("com.ostrich.connect.linux")
        .build();

    app.connect_activate(build_ui);
    app.run();
}
