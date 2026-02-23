use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use oc_backend::{Backend, ProtocolRegistry};
use oc_core::command::{UiCommand, UiResponse};
use oc_core::types::{
    ConnectionProfile, ConnectionSecurity, ProtocolKind, RemoteEntry, RemoteEntryKind, SessionId,
};
use oc_protocol_ftp::FtpProtocolFactory;
use oc_protocol_ftps::FtpsProtocolFactory;
use oc_protocol_sftp::SftpProtocolFactory;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Terminal;
use secrecy::{ExposeSecret, SecretString};

#[derive(Clone, Copy)]
struct UiTheme {
    title: Color,
    border: Color,
    accent: Color,
    directory: Color,
    symlink: Color,
    success: Color,
    warning: Color,
    error: Color,
    muted: Color,
}

impl Default for UiTheme {
    fn default() -> Self {
        // ANSI palette colors are terminal-theme aware.
        Self {
            title: Color::Cyan,
            border: Color::DarkGray,
            accent: Color::Blue,
            directory: Color::Cyan,
            symlink: Color::Magenta,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            muted: Color::DarkGray,
        }
    }
}

#[derive(Clone)]
struct SavedConnection {
    name: String,
    profile: ConnectionProfile,
}

#[derive(Clone)]
struct DownloadPopup {
    entry: RemoteEntry,
    local_path: String,
}

#[derive(Clone, Copy)]
enum FormMode {
    Create,
    Edit(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FormField {
    Name,
    Protocol,
    Host,
    Port,
    Username,
    Password,
    PrivateKeyPath,
    PrivateKeyPem,
    Security,
    StrictHostKeyChecking,
    PassiveMode,
    Submit,
    Cancel,
}

impl FormField {
    const ORDER: [FormField; 13] = [
        FormField::Name,
        FormField::Protocol,
        FormField::Host,
        FormField::Port,
        FormField::Username,
        FormField::Password,
        FormField::PrivateKeyPath,
        FormField::PrivateKeyPem,
        FormField::Security,
        FormField::StrictHostKeyChecking,
        FormField::PassiveMode,
        FormField::Submit,
        FormField::Cancel,
    ];

    fn index(self) -> usize {
        Self::ORDER
            .iter()
            .position(|field| *field == self)
            .unwrap_or(0)
    }

    fn next(self) -> Self {
        let index = self.index();
        let next = (index + 1) % Self::ORDER.len();
        Self::ORDER[next]
    }

    fn prev(self) -> Self {
        let index = self.index();
        let prev = if index == 0 {
            Self::ORDER.len() - 1
        } else {
            index - 1
        };
        Self::ORDER[prev]
    }
}

struct ConnectionForm {
    mode: FormMode,
    field: FormField,
    name: String,
    protocol: ProtocolKind,
    host: String,
    port: String,
    username: String,
    password: String,
    private_key_path: String,
    private_key_pem: String,
    security: ConnectionSecurity,
    strict_host_key_checking: bool,
    passive_mode: bool,
    error: Option<String>,
}

impl ConnectionForm {
    fn new_create() -> Self {
        let (port, security, passive_mode, strict_host_key_checking) =
            protocol_defaults(ProtocolKind::Sftp);
        Self {
            mode: FormMode::Create,
            field: FormField::Name,
            name: String::new(),
            protocol: ProtocolKind::Sftp,
            host: String::new(),
            port: port.to_string(),
            username: String::new(),
            password: String::new(),
            private_key_path: String::new(),
            private_key_pem: String::new(),
            security,
            strict_host_key_checking,
            passive_mode,
            error: None,
        }
    }

    fn from_existing(index: usize, connection: SavedConnection) -> Self {
        Self {
            mode: FormMode::Edit(index),
            field: FormField::Name,
            name: connection.name,
            protocol: connection.profile.protocol,
            host: connection.profile.host,
            port: connection.profile.port.to_string(),
            username: connection.profile.username,
            password: connection
                .profile
                .password
                .map(|secret| secret.expose_secret().to_owned())
                .unwrap_or_default(),
            private_key_path: connection.profile.private_key_path.unwrap_or_default(),
            private_key_pem: connection
                .profile
                .private_key_pem
                .map(|secret| secret.expose_secret().to_owned())
                .unwrap_or_default(),
            security: connection.profile.security,
            strict_host_key_checking: connection.profile.strict_host_key_checking,
            passive_mode: connection.profile.passive_mode,
            error: None,
        }
    }

    fn set_protocol(&mut self, protocol: ProtocolKind) {
        self.protocol = protocol;
        let (port, security, passive_mode, strict_host_key_checking) = protocol_defaults(protocol);
        self.port = port.to_string();
        self.security = security;
        self.passive_mode = passive_mode;
        self.strict_host_key_checking = strict_host_key_checking;
    }

    fn cycle_protocol(&mut self, forward: bool) {
        let next = match (self.protocol, forward) {
            (ProtocolKind::Ftp, true) => ProtocolKind::Sftp,
            (ProtocolKind::Sftp, true) => ProtocolKind::Ftps,
            (ProtocolKind::Ftps, true) => ProtocolKind::Ftp,
            (ProtocolKind::Ftp, false) => ProtocolKind::Ftps,
            (ProtocolKind::Sftp, false) => ProtocolKind::Ftp,
            (ProtocolKind::Ftps, false) => ProtocolKind::Sftp,
        };
        self.set_protocol(next);
    }

    fn cycle_security(&mut self) {
        self.security = match self.protocol {
            ProtocolKind::Ftp => ConnectionSecurity::PlainText,
            ProtocolKind::Sftp => ConnectionSecurity::SshTransport,
            ProtocolKind::Ftps => match self.security {
                ConnectionSecurity::TlsExplicit => ConnectionSecurity::TlsImplicit,
                _ => ConnectionSecurity::TlsExplicit,
            },
        };
    }

    fn toggle_boolean_field(&mut self) {
        match self.field {
            FormField::StrictHostKeyChecking => {
                self.strict_host_key_checking = !self.strict_host_key_checking;
            }
            FormField::PassiveMode => {
                self.passive_mode = !self.passive_mode;
            }
            _ => {}
        }
    }

    fn active_text_mut(&mut self) -> Option<&mut String> {
        match self.field {
            FormField::Name => Some(&mut self.name),
            FormField::Host => Some(&mut self.host),
            FormField::Port => Some(&mut self.port),
            FormField::Username => Some(&mut self.username),
            FormField::Password => Some(&mut self.password),
            FormField::PrivateKeyPath => Some(&mut self.private_key_path),
            FormField::PrivateKeyPem => Some(&mut self.private_key_pem),
            _ => None,
        }
    }

    fn build_connection(&self) -> Result<SavedConnection, String> {
        if self.name.trim().is_empty() {
            return Err("Connection name is required.".to_owned());
        }
        if self.host.trim().is_empty() {
            return Err("Host is required.".to_owned());
        }
        if self.username.trim().is_empty() {
            return Err("Username is required.".to_owned());
        }
        let port: u16 = self
            .port
            .trim()
            .parse()
            .map_err(|_| "Port must be a number between 1 and 65535.".to_owned())?;

        let password = if self.password.is_empty() {
            None
        } else {
            Some(SecretString::new(self.password.clone()))
        };
        let private_key_path = if self.private_key_path.trim().is_empty() {
            None
        } else {
            Some(self.private_key_path.trim().to_owned())
        };
        let private_key_pem = if self.private_key_pem.trim().is_empty() {
            None
        } else {
            Some(SecretString::new(self.private_key_pem.clone()))
        };

        let profile = ConnectionProfile {
            protocol: self.protocol,
            host: self.host.trim().to_owned(),
            port,
            username: self.username.trim().to_owned(),
            password,
            private_key_pem,
            private_key_path,
            security: self.security,
            strict_host_key_checking: self.strict_host_key_checking,
            passive_mode: self.passive_mode,
        };

        profile.validate().map_err(|error| error.to_string())?;

        Ok(SavedConnection {
            name: self.name.trim().to_owned(),
            profile,
        })
    }
}

enum Screen {
    Connections,
    Form(ConnectionForm),
    Navigator,
}

#[derive(Clone, Copy)]
enum ScreenKind {
    Connections,
    Form,
    Navigator,
}

enum FormAction {
    None,
    Cancel,
    Save {
        mode: FormMode,
        connection: Result<SavedConnection, String>,
    },
}

struct AppState {
    backend: Backend,
    theme: UiTheme,
    screen: Screen,
    should_quit: bool,
    status: String,
    connections: Vec<SavedConnection>,
    selected_connection: usize,
    session_id: Option<SessionId>,
    current_path: String,
    entries: Vec<RemoteEntry>,
    selected_entry: usize,
    navigator_search: Option<String>,
    download_popup: Option<DownloadPopup>,
}

impl AppState {
    fn new() -> Self {
        Self {
            backend: build_backend(),
            theme: UiTheme::default(),
            screen: Screen::Connections,
            should_quit: false,
            status: "Create a connection with n, then press Enter to open it.".to_owned(),
            connections: Vec::new(),
            selected_connection: 0,
            session_id: None,
            current_path: "/".to_owned(),
            entries: Vec::new(),
            selected_entry: 0,
            navigator_search: None,
            download_popup: None,
        }
    }

    fn screen_kind(&self) -> ScreenKind {
        match self.screen {
            Screen::Connections => ScreenKind::Connections,
            Screen::Form(_) => ScreenKind::Form,
            Screen::Navigator => ScreenKind::Navigator,
        }
    }

    fn selected_connection(&self) -> Option<&SavedConnection> {
        self.connections.get(self.selected_connection)
    }

    fn selected_entry(&self) -> Option<&RemoteEntry> {
        self.entries.get(self.selected_entry)
    }

    fn move_connection_cursor(&mut self, direction: isize) {
        if self.connections.is_empty() {
            self.selected_connection = 0;
            return;
        }
        if direction > 0 {
            self.selected_connection = (self.selected_connection + 1) % self.connections.len();
        } else if self.selected_connection == 0 {
            self.selected_connection = self.connections.len() - 1;
        } else {
            self.selected_connection -= 1;
        }
    }

    fn move_entry_cursor(&mut self, direction: isize) {
        if self.entries.is_empty() {
            self.selected_entry = 0;
            return;
        }
        if direction > 0 {
            self.selected_entry = (self.selected_entry + 1) % self.entries.len();
        } else if self.selected_entry == 0 {
            self.selected_entry = self.entries.len() - 1;
        } else {
            self.selected_entry -= 1;
        }
    }

    fn move_entry_cursor_many(&mut self, amount: isize) {
        if amount == 0 {
            return;
        }
        let direction = if amount > 0 { 1 } else { -1 };
        for _ in 0..amount.unsigned_abs() {
            self.move_entry_cursor(direction);
        }
    }

    fn jump_to_first_entry(&mut self) {
        if !self.entries.is_empty() {
            self.selected_entry = 0;
        }
    }

    fn jump_to_last_entry(&mut self) {
        if !self.entries.is_empty() {
            self.selected_entry = self.entries.len() - 1;
        }
    }

    fn navigator_search_query(&self) -> &str {
        self.navigator_search.as_deref().unwrap_or_default().trim()
    }

    fn directory_prefix_matches(&self, query: &str) -> Vec<usize> {
        let query_lower = query.to_lowercase();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                entry.kind == RemoteEntryKind::Directory
                    && entry.name.to_lowercase().starts_with(&query_lower)
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn enter_search_mode(&mut self) {
        self.navigator_search = Some(String::new());
        self.status = "Search mode: type a folder prefix, Esc to cancel.".to_owned();
    }

    fn stop_search_mode(&mut self, message: &str) {
        self.navigator_search = None;
        self.status = message.to_owned();
    }

    async fn update_search(&mut self) {
        let query = self.navigator_search_query().to_owned();
        if self.navigator_search.is_none() {
            return;
        }
        if query.is_empty() {
            self.status = "Search: /".to_owned();
            return;
        }

        let matches = self.directory_prefix_matches(&query);
        match matches.len() {
            0 => {
                self.status = format!("Search '/{query}': no directory match.");
            }
            1 => {
                let index = matches[0];
                self.selected_entry = index;
                self.navigator_search = None;
                self.open_selected_entry().await;
            }
            count => {
                self.selected_entry = matches[0];
                self.status = format!("Search '/{query}': {count} matches, keep typing.");
            }
        }
    }

    async fn handle_navigator_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.stop_search_mode("Search canceled."),
            KeyCode::Enter => {
                self.update_search().await;
            }
            KeyCode::Backspace => {
                if let Some(query) = &mut self.navigator_search {
                    query.pop();
                }
                self.update_search().await;
            }
            KeyCode::Char(ch) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT)
                {
                    if let Some(query) = &mut self.navigator_search {
                        query.push(ch);
                    }
                    self.update_search().await;
                }
            }
            _ => {}
        }
    }

    async fn handle_key(&mut self, key: KeyEvent) {
        if self.download_popup.is_some() {
            self.handle_download_popup_key(key).await;
            return;
        }

        match self.screen_kind() {
            ScreenKind::Connections => self.handle_connections_key(key).await,
            ScreenKind::Form => self.handle_form_key(key),
            ScreenKind::Navigator => self.handle_navigator_key(key).await,
        }
    }

    async fn handle_connections_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('n') => self.screen = Screen::Form(ConnectionForm::new_create()),
            KeyCode::Char('e') => self.open_edit_form(),
            KeyCode::Char('x') | KeyCode::Delete => self.delete_selected_connection(),
            KeyCode::Char('j') | KeyCode::Down => self.move_connection_cursor(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_connection_cursor(-1),
            KeyCode::Enter => self.connect_selected().await,
            _ => {}
        }
    }

    fn handle_form_key(&mut self, key: KeyEvent) {
        let mut action = FormAction::None;

        if let Screen::Form(form) = &mut self.screen {
            form.error = None;
            match key.code {
                KeyCode::Esc => action = FormAction::Cancel,
                KeyCode::Tab | KeyCode::Down => form.field = form.field.next(),
                KeyCode::BackTab | KeyCode::Up => form.field = form.field.prev(),
                KeyCode::Left => {
                    if form.field == FormField::Protocol {
                        form.cycle_protocol(false);
                    }
                }
                KeyCode::Right => {
                    if form.field == FormField::Protocol {
                        form.cycle_protocol(true);
                    }
                }
                KeyCode::Enter => match form.field {
                    FormField::Submit => {
                        action = FormAction::Save {
                            mode: form.mode,
                            connection: form.build_connection(),
                        };
                    }
                    FormField::Cancel => action = FormAction::Cancel,
                    FormField::Protocol => form.cycle_protocol(true),
                    FormField::Security => form.cycle_security(),
                    FormField::StrictHostKeyChecking | FormField::PassiveMode => {
                        form.toggle_boolean_field();
                    }
                    _ => form.field = form.field.next(),
                },
                KeyCode::Char(' ') => match form.field {
                    FormField::Protocol => form.cycle_protocol(true),
                    FormField::Security => form.cycle_security(),
                    FormField::StrictHostKeyChecking | FormField::PassiveMode => {
                        form.toggle_boolean_field();
                    }
                    _ => {}
                },
                KeyCode::Backspace => {
                    if let Some(active_text) = form.active_text_mut() {
                        active_text.pop();
                    }
                }
                KeyCode::Char(ch) => {
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT)
                    {
                        if let Some(active_text) = form.active_text_mut() {
                            active_text.push(ch);
                        }
                    }
                }
                _ => {}
            }
        }

        match action {
            FormAction::None => {}
            FormAction::Cancel => {
                self.screen = Screen::Connections;
                self.status = "Connection edit canceled.".to_owned();
            }
            FormAction::Save { mode, connection } => match connection {
                Ok(connection) => {
                    match mode {
                        FormMode::Create => {
                            self.connections.push(connection);
                            self.selected_connection = self.connections.len().saturating_sub(1);
                            self.status = "Connection created.".to_owned();
                        }
                        FormMode::Edit(index) => {
                            if index < self.connections.len() {
                                self.connections[index] = connection;
                                self.selected_connection = index;
                                self.status = "Connection updated.".to_owned();
                            } else {
                                self.connections.push(connection);
                                self.selected_connection = self.connections.len().saturating_sub(1);
                                self.status = "Connection created.".to_owned();
                            }
                        }
                    }
                    self.screen = Screen::Connections;
                }
                Err(message) => {
                    if let Screen::Form(form) = &mut self.screen {
                        form.error = Some(message);
                    }
                }
            },
        }
    }

    async fn handle_navigator_key(&mut self, key: KeyEvent) {
        if self.navigator_search.is_some() {
            self.handle_navigator_search_key(key).await;
            return;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('d') => {
                    self.move_entry_cursor_many(10);
                    return;
                }
                KeyCode::Char('u') => {
                    self.move_entry_cursor_many(-10);
                    return;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Char('b') | KeyCode::Esc => {
                self.disconnect_to_connections().await;
            }
            KeyCode::Char('j') | KeyCode::Down => self.move_entry_cursor(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_entry_cursor(-1),
            KeyCode::Char('g') => self.jump_to_first_entry(),
            KeyCode::Char('G') => self.jump_to_last_entry(),
            KeyCode::Char('h') | KeyCode::Backspace => {
                self.current_path = parent_remote_path(&self.current_path);
                self.selected_entry = 0;
                self.refresh_directory().await;
            }
            KeyCode::Char('-') => {
                self.current_path = parent_remote_path(&self.current_path);
                self.selected_entry = 0;
                self.refresh_directory().await;
            }
            KeyCode::Char('~') => {
                self.current_path = "/".to_owned();
                self.selected_entry = 0;
                self.refresh_directory().await;
            }
            KeyCode::Char('l') | KeyCode::Enter => self.open_selected_entry().await,
            KeyCode::Char('r') => self.refresh_directory().await,
            KeyCode::Char('d') => self.open_download_popup(),
            KeyCode::Char('/') => self.enter_search_mode(),
            _ => {}
        }
    }

    async fn handle_download_popup_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => self.confirm_download().await,
            KeyCode::Char('n') | KeyCode::Esc => {
                self.download_popup = None;
                self.status = "Download canceled.".to_owned();
            }
            _ => {}
        }
    }

    fn open_edit_form(&mut self) {
        let Some(connection) = self.selected_connection().cloned() else {
            self.status = "No connection selected.".to_owned();
            return;
        };
        let index = self.selected_connection;
        self.screen = Screen::Form(ConnectionForm::from_existing(index, connection));
    }

    fn delete_selected_connection(&mut self) {
        if self.connections.is_empty() {
            self.status = "No connection to delete.".to_owned();
            return;
        }

        self.connections.remove(self.selected_connection);
        if self.selected_connection >= self.connections.len() && !self.connections.is_empty() {
            self.selected_connection = self.connections.len() - 1;
        }
        if self.connections.is_empty() {
            self.selected_connection = 0;
        }
        self.status = "Connection deleted.".to_owned();
    }

    async fn connect_selected(&mut self) {
        let Some(connection) = self.selected_connection().cloned() else {
            self.status = "No saved connection. Press n to create one.".to_owned();
            return;
        };

        let response = self
            .backend
            .execute(UiCommand::Connect {
                profile: connection.profile,
            })
            .await;

        match response {
            UiResponse::Connected {
                session_id,
                peer,
                protocol,
            } => {
                self.session_id = Some(session_id);
                self.current_path = "/".to_owned();
                self.entries.clear();
                self.selected_entry = 0;
                self.navigator_search = None;
                self.download_popup = None;
                self.screen = Screen::Navigator;
                self.status = format!("Connected to {peer} via {protocol}.");
                self.refresh_directory().await;
            }
            UiResponse::Error { message, .. } => {
                self.status = format!("Connection failed: {message}");
            }
            other => {
                self.status = format!("Unexpected backend response: {other:?}");
            }
        }
    }

    async fn disconnect_to_connections(&mut self) {
        if let Some(session_id) = self.session_id.take() {
            let response = self
                .backend
                .execute(UiCommand::Disconnect { session_id })
                .await;
            match response {
                UiResponse::Disconnected { .. } => {
                    self.status = "Disconnected.".to_owned();
                }
                UiResponse::Error { message, .. } => {
                    self.status = format!("Disconnect error: {message}");
                }
                _ => {
                    self.status = "Disconnected.".to_owned();
                }
            }
        }

        self.screen = Screen::Connections;
        self.current_path = "/".to_owned();
        self.entries.clear();
        self.selected_entry = 0;
        self.navigator_search = None;
        self.download_popup = None;
    }

    async fn refresh_directory(&mut self) {
        let Some(session_id) = self.session_id else {
            self.status = "No active session.".to_owned();
            return;
        };

        let path = self.current_path.clone();
        let response = self
            .backend
            .execute(UiCommand::ListDirectory {
                session_id,
                path: path.clone(),
            })
            .await;

        match response {
            UiResponse::Directory { mut entries, .. } => {
                sort_entries(&mut entries);
                self.entries = entries;
                if self.selected_entry >= self.entries.len() && !self.entries.is_empty() {
                    self.selected_entry = self.entries.len() - 1;
                }
                if self.entries.is_empty() {
                    self.selected_entry = 0;
                }
                self.status = format!("{} entries in {}", self.entries.len(), path);
            }
            UiResponse::Error { message, .. } => {
                self.status = format!("List failed: {message}");
            }
            other => {
                self.status = format!("Unexpected backend response: {other:?}");
            }
        }
    }

    async fn open_selected_entry(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            self.status = "No entry selected.".to_owned();
            return;
        };

        if entry.kind != RemoteEntryKind::Directory {
            self.status = "Selected entry is not a directory.".to_owned();
            return;
        }

        self.current_path = normalize_remote_path(&entry.path);
        self.selected_entry = 0;
        self.refresh_directory().await;
    }

    fn open_download_popup(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            self.status = "No entry selected.".to_owned();
            return;
        };

        if entry.kind == RemoteEntryKind::Directory {
            self.status = "Cannot download a directory in this scaffold.".to_owned();
            return;
        }

        let local_path = downloads_target_for(&entry.name);
        self.download_popup = Some(DownloadPopup { entry, local_path });
    }

    async fn confirm_download(&mut self) {
        let Some(popup) = self.download_popup.clone() else {
            return;
        };
        self.download_popup = None;

        let Some(session_id) = self.session_id else {
            self.status = "No active session.".to_owned();
            return;
        };

        if let Some(parent_dir) = Path::new(&popup.local_path).parent() {
            if let Err(error) = std::fs::create_dir_all(parent_dir) {
                self.status = format!("Could not prepare Downloads folder: {error}");
                return;
            }
        }

        let response = self
            .backend
            .execute(UiCommand::DownloadFile {
                session_id,
                remote_path: popup.entry.path.clone(),
                local_path: popup.local_path.clone(),
            })
            .await;

        match response {
            UiResponse::TransferCompleted { destination, .. } => {
                self.status = format!("Downloaded to {destination}");
            }
            UiResponse::Error { message, .. } => {
                self.status = format!("Download failed: {message}");
            }
            other => {
                self.status = format!("Unexpected backend response: {other:?}");
            }
        }
    }
}

fn protocol_defaults(protocol: ProtocolKind) -> (u16, ConnectionSecurity, bool, bool) {
    match protocol {
        ProtocolKind::Ftp => (21, ConnectionSecurity::PlainText, true, false),
        ProtocolKind::Sftp => (22, ConnectionSecurity::SshTransport, false, true),
        ProtocolKind::Ftps => (21, ConnectionSecurity::TlsExplicit, true, false),
    }
}

fn security_label(security: ConnectionSecurity) -> &'static str {
    match security {
        ConnectionSecurity::PlainText => "plain_text",
        ConnectionSecurity::TlsExplicit => "tls_explicit",
        ConnectionSecurity::TlsImplicit => "tls_implicit",
        ConnectionSecurity::SshTransport => "ssh_transport",
    }
}

fn normalize_remote_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "." {
        return "/".to_owned();
    }
    if trimmed.starts_with('/') {
        trimmed.to_owned()
    } else {
        format!("/{}", trimmed)
    }
}

fn parent_remote_path(path: &str) -> String {
    let current = normalize_remote_path(path);
    if current == "/" {
        return current;
    }
    let trimmed = current.trim_end_matches('/');
    if let Some(index) = trimmed.rfind('/') {
        if index == 0 {
            "/".to_owned()
        } else {
            trimmed[..index].to_owned()
        }
    } else {
        "/".to_owned()
    }
}

fn sort_entries(entries: &mut Vec<RemoteEntry>) {
    entries.sort_by(|left, right| {
        let left_rank = match left.kind {
            RemoteEntryKind::Directory => 0,
            RemoteEntryKind::Symlink => 1,
            RemoteEntryKind::File => 2,
        };
        let right_rank = match right.kind {
            RemoteEntryKind::Directory => 0,
            RemoteEntryKind::Symlink => 1,
            RemoteEntryKind::File => 2,
        };
        left_rank
            .cmp(&right_rank)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
}

fn downloads_target_for(file_name: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_owned());
    let downloads = PathBuf::from(home).join("Downloads").join(file_name);
    downloads.to_string_lossy().to_string()
}

fn build_backend() -> Backend {
    let mut registry = ProtocolRegistry::default();
    registry.register(FtpProtocolFactory::new());
    registry.register(SftpProtocolFactory::new());
    registry.register(FtpsProtocolFactory::new());
    Backend::new(registry)
}

fn themed_block<'a>(title: &'a str, theme: UiTheme) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(title)
        .title_style(
            Style::default()
                .fg(theme.title)
                .add_modifier(Modifier::BOLD),
        )
}

fn status_style(status: &str, theme: UiTheme) -> Style {
    let status_lower = status.to_lowercase();
    if status_lower.contains("failed")
        || status_lower.contains("error")
        || status_lower.contains("mismatch")
    {
        Style::default()
            .fg(theme.error)
            .add_modifier(Modifier::BOLD)
    } else if status_lower.contains("connected")
        || status_lower.contains("downloaded")
        || status_lower.contains("created")
        || status_lower.contains("updated")
    {
        Style::default().fg(theme.success)
    } else if status_lower.contains("search")
        || status_lower.contains("canceled")
        || status_lower.contains("cancelled")
    {
        Style::default().fg(theme.warning)
    } else {
        Style::default().fg(theme.muted)
    }
}

fn protocol_color(protocol: ProtocolKind, theme: UiTheme) -> Color {
    match protocol {
        ProtocolKind::Ftp => theme.warning,
        ProtocolKind::Ftps => theme.success,
        ProtocolKind::Sftp => theme.accent,
    }
}

fn draw(frame: &mut ratatui::Frame, app: &AppState) {
    match &app.screen {
        Screen::Connections => draw_connections(frame, app),
        Screen::Form(form) => draw_connection_form(frame, app, form),
        Screen::Navigator => draw_navigator(frame, app),
    }

    if let Some(popup) = &app.download_popup {
        draw_download_popup(frame, popup, app.theme);
    }
}

fn draw_connections(frame: &mut ratatui::Frame, app: &AppState) {
    let theme = app.theme;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let header = Paragraph::new("Connection Manager")
        .style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .block(themed_block("ostrich-connect", theme));
    frame.render_widget(header, chunks[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(chunks[1]);

    let list_items = if app.connections.is_empty() {
        vec![
            ListItem::new("No saved connections. Press n to create one.")
                .style(Style::default().fg(theme.muted)),
        ]
    } else {
        app.connections
            .iter()
            .map(|connection| {
                ListItem::new(format!(
                    "{} [{}] {}:{}",
                    connection.name,
                    connection.profile.protocol,
                    connection.profile.host,
                    connection.profile.port
                ))
                .style(Style::default().fg(protocol_color(connection.profile.protocol, theme)))
            })
            .collect()
    };

    let list = List::new(list_items)
        .block(themed_block("Connections", theme))
        .highlight_symbol("> ")
        .highlight_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::REVERSED | Modifier::BOLD),
        );

    let mut state = ListState::default();
    if !app.connections.is_empty() {
        state.select(Some(app.selected_connection));
    }
    frame.render_stateful_widget(list, body[0], &mut state);

    let details = if let Some(connection) = app.selected_connection() {
        let masked = "*".repeat(connection.profile.password.as_ref().map(|_| 8).unwrap_or(0));
        let key_path = connection
            .profile
            .private_key_path
            .as_deref()
            .unwrap_or("-");
        let key_pem = if connection.profile.private_key_pem.is_some() {
            "<provided>"
        } else {
            "-"
        };
        format!(
            "Name: {}\nProtocol: {}\nHost: {}\nPort: {}\nUsername: {}\nPassword: {}\nPrivate key path: {}\nPrivate key pem: {}\nSecurity: {}\nStrict host key checking: {}\nPassive mode: {}",
            connection.name,
            connection.profile.protocol,
            connection.profile.host,
            connection.profile.port,
            connection.profile.username,
            masked,
            key_path,
            key_pem,
            security_label(connection.profile.security),
            connection.profile.strict_host_key_checking,
            connection.profile.passive_mode
        )
    } else {
        "Select a connection once one exists.".to_owned()
    };
    let details_widget = Paragraph::new(details)
        .style(Style::default().fg(theme.muted))
        .block(themed_block("Details", theme));
    frame.render_widget(details_widget, body[1]);

    let footer = Paragraph::new(format!(
        "{} | n:new e:edit x:delete Enter:open j/k or arrows:move q:quit",
        app.status
    ))
    .style(status_style(&app.status, theme))
    .block(themed_block("Keys", theme));
    frame.render_widget(footer, chunks[2]);
}

fn draw_connection_form(frame: &mut ratatui::Frame, app: &AppState, form: &ConnectionForm) {
    let theme = app.theme;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(4),
        ])
        .split(frame.area());

    let title = match form.mode {
        FormMode::Create => "Create Connection",
        FormMode::Edit(_) => "Edit Connection",
    };
    let header = Paragraph::new(title)
        .style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .block(themed_block("Form", theme));
    frame.render_widget(header, chunks[0]);

    let password_mask = "*".repeat(form.password.len());
    let private_key_pem_label = if form.private_key_pem.is_empty() {
        String::new()
    } else {
        "<provided>".to_owned()
    };
    let fields = vec![
        ("Name", form.name.clone()),
        ("Protocol", form.protocol.to_string()),
        ("Host", form.host.clone()),
        ("Port", form.port.clone()),
        ("Username", form.username.clone()),
        ("Password", password_mask),
        ("Private Key Path", form.private_key_path.clone()),
        ("Private Key PEM", private_key_pem_label),
        ("Security", security_label(form.security).to_owned()),
        (
            "Strict Host Key Checking",
            form.strict_host_key_checking.to_string(),
        ),
        ("Passive Mode", form.passive_mode.to_string()),
        ("Save", "Press Enter".to_owned()),
        ("Cancel", "Press Enter".to_owned()),
    ];

    let list_items: Vec<ListItem> = fields
        .into_iter()
        .map(|(label, value)| ListItem::new(format!("{label:25} {value}")))
        .collect();

    let list = List::new(list_items)
        .block(themed_block("Tab/Shift+Tab to move, type to edit", theme))
        .highlight_symbol(">> ")
        .highlight_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::REVERSED | Modifier::BOLD),
        );

    let mut state = ListState::default();
    state.select(Some(form.field.index()));
    frame.render_stateful_widget(list, chunks[1], &mut state);

    let error_line = form
        .error
        .as_ref()
        .map(|error| format!("Form error: {error}"))
        .unwrap_or_else(|| app.status.clone());

    let footer = Paragraph::new(format!(
        "{} | Enter:apply field Space:toggle bool/protocol/security Esc:cancel",
        error_line
    ))
    .style(status_style(&error_line, theme))
    .block(themed_block("Status", theme));
    frame.render_widget(footer, chunks[2]);
}

fn draw_navigator(frame: &mut ratatui::Frame, app: &AppState) {
    let theme = app.theme;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let query = app.navigator_search_query().to_owned();
    let search_marker = if app.navigator_search.is_some() {
        format!(" | /{query}")
    } else {
        String::new()
    };
    let top_line = format!(
        "path: {}{} | {}",
        app.current_path, search_marker, app.status
    );
    frame.render_widget(
        Paragraph::new(top_line).style(status_style(&app.status, theme)),
        chunks[0],
    );

    let entries = if app.entries.is_empty() {
        vec![ListItem::new("No entries").style(Style::default().fg(theme.muted))]
    } else {
        app.entries
            .iter()
            .map(|entry| {
                let kind = match entry.kind {
                    RemoteEntryKind::Directory => "d",
                    RemoteEntryKind::File => "-",
                    RemoteEntryKind::Symlink => "l",
                };
                let name = if entry.kind == RemoteEntryKind::Directory {
                    format!("{}/", entry.name)
                } else {
                    entry.name.clone()
                };
                let size = if entry.kind == RemoteEntryKind::Directory {
                    "-".to_owned()
                } else {
                    entry.size.to_string()
                };
                let mut style = match entry.kind {
                    RemoteEntryKind::Directory => Style::default().fg(theme.directory),
                    RemoteEntryKind::Symlink => Style::default().fg(theme.symlink),
                    RemoteEntryKind::File => Style::default(),
                };

                if app.navigator_search.is_some()
                    && !query.is_empty()
                    && entry.kind == RemoteEntryKind::Directory
                    && entry.name.to_lowercase().starts_with(&query.to_lowercase())
                {
                    style = style.add_modifier(Modifier::UNDERLINED | Modifier::BOLD);
                }

                ListItem::new(format!("{kind} {name:<48} {size:>12}")).style(style)
            })
            .collect()
    };

    let list = List::new(entries).highlight_symbol("> ").highlight_style(
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::REVERSED | Modifier::BOLD),
    );
    let mut state = ListState::default();
    if !app.entries.is_empty() {
        state.select(Some(app.selected_entry));
    }
    frame.render_stateful_widget(list, chunks[1], &mut state);

    let footer = if app.navigator_search.is_some() {
        "/ search: type prefix | Enter:resolve Esc:cancel Backspace:edit"
    } else {
        "j/k:move h/l:up/open g/G:top/bottom Ctrl-u/d:jump /:search d:download r:refresh b/q:back"
    };
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().fg(theme.muted)),
        chunks[2],
    );
}

fn draw_download_popup(frame: &mut ratatui::Frame, popup: &DownloadPopup, theme: UiTheme) {
    let area = centered_rect(70, 35, frame.area());
    frame.render_widget(Clear, area);

    let lines = vec![
        Line::from("Download selected file to Downloads folder?"),
        Line::from(""),
        Line::from(format!("Remote: {}", popup.entry.path)),
        Line::from(format!("Local: {}", popup.local_path)),
        Line::from(""),
        Line::from("y: yes    n: no"),
    ];
    let widget = Paragraph::new(lines)
        .style(Style::default().fg(theme.warning))
        .block(themed_block("Confirm Download", theme));
    frame.render_widget(widget, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    enable_raw_mode()?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = AppState::new();

    loop {
        terminal.draw(|frame| draw(frame, &app))?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        app.should_quit = true;
                    } else {
                        app.handle_key(key).await;
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
