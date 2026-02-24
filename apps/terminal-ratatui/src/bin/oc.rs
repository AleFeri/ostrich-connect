use std::env;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use oc_backend::{Backend, ProtocolRegistry};
use oc_core::command::{UiCommand, UiResponse};
use oc_core::types::SavedConnection;
use oc_protocol_ftp::FtpProtocolFactory;
use oc_protocol_ftps::FtpsProtocolFactory;
use oc_protocol_sftp::SftpProtocolFactory;

#[derive(Parser)]
#[command(
    name = "oc",
    version,
    about = "ostrich-connect CLI",
    long_about = "Use `oc` to open the TUI or run quick actions against backend-managed config."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// List saved connections from backend config.
    Ls,
    /// Open ratatui and auto-connect to a saved connection.
    Connect {
        /// Saved connection name from `oc ls`.
        connection_name: String,
    },
    /// Print shell completion script.
    Completion {
        /// Shell to generate completion for.
        shell: Shell,
    },
    #[command(name = "__complete_connections", hide = true)]
    CompleteConnections {
        /// Optional prefix to filter names.
        prefix: Option<String>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Shell {
    Bash,
    Zsh,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None => launch_tui(None),
        Some(Commands::Ls) => {
            let connections = load_connections().await?;
            for connection in connections {
                println!("{}", connection.name);
            }
            Ok(())
        }
        Some(Commands::Connect { connection_name }) => {
            let connections = load_connections().await?;
            let connection = resolve_connection(&connections, &connection_name)?;
            launch_tui(Some(&connection.name))
        }
        Some(Commands::Completion { shell }) => {
            print_completion_script(shell);
            Ok(())
        }
        Some(Commands::CompleteConnections { prefix }) => {
            let connections = load_connections().await?;
            let prefix = prefix.unwrap_or_default().to_lowercase();
            for connection in connections {
                if prefix.is_empty() || connection.name.to_lowercase().starts_with(&prefix) {
                    println!("{}", connection.name);
                }
            }
            Ok(())
        }
    }
}

fn launch_tui(connection_name: Option<&str>) -> Result<()> {
    let tui_binary = tui_binary_path();
    let mut command = Command::new(&tui_binary);
    if let Some(connection_name) = connection_name {
        command.env("OSTRICH_CONNECT_AUTO_CONNECT", connection_name);
    }

    let status = command
        .status()
        .with_context(|| format!("failed to start {}", tui_binary.display()))?;

    if status.success() {
        return Ok(());
    }

    let code = status
        .code()
        .map(|value| value.to_string())
        .unwrap_or_else(|| "signal".to_owned());
    bail!("terminal-ratatui exited with status {code}")
}

fn tui_binary_path() -> PathBuf {
    if let Ok(custom) = env::var("OSTRICH_CONNECT_TUI_BIN") {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    let binary_name = if cfg!(windows) {
        "terminal-ratatui.exe"
    } else {
        "terminal-ratatui"
    };

    if let Ok(current_exe) = env::current_exe() {
        let candidate = current_exe.with_file_name(binary_name);
        if candidate.exists() {
            return candidate;
        }
    }

    PathBuf::from(binary_name)
}

async fn load_connections() -> Result<Vec<SavedConnection>> {
    let mut backend = build_backend();
    let response = backend.execute(UiCommand::LoadConfig).await;
    match response {
        UiResponse::Config { config } => Ok(config.connections),
        UiResponse::Error { message, .. } => Err(anyhow!("could not load config: {message}")),
        other => Err(anyhow!("unexpected backend response: {other:?}")),
    }
}

fn resolve_connection<'a>(
    connections: &'a [SavedConnection],
    query: &str,
) -> Result<&'a SavedConnection> {
    if let Some(exact) = connections
        .iter()
        .find(|connection| connection.name.eq_ignore_ascii_case(query))
    {
        return Ok(exact);
    }

    let query_lower = query.to_lowercase();
    let suggestions: Vec<&str> = connections
        .iter()
        .filter(|connection| connection.name.to_lowercase().starts_with(&query_lower))
        .map(|connection| connection.name.as_str())
        .collect();

    if suggestions.is_empty() {
        return Err(anyhow!(
            "connection '{query}' not found. Use `oc ls` to list names."
        ));
    }

    Err(anyhow!(
        "connection '{query}' not found. Did you mean: {}",
        suggestions.join(", ")
    ))
}

fn build_backend() -> Backend {
    let mut registry = ProtocolRegistry::default();
    registry.register(FtpProtocolFactory::new());
    registry.register(SftpProtocolFactory::new());
    registry.register(FtpsProtocolFactory::new());
    Backend::new(registry)
}

fn print_completion_script(shell: Shell) {
    match shell {
        Shell::Zsh => print_zsh_completion(),
        Shell::Bash => print_bash_completion(),
    }
}

fn print_zsh_completion() {
    println!(
        r#"#compdef oc

_oc() {{
  local state
  typeset -a commands
  commands=(
    "ls:List saved connections"
    "connect:Open ratatui and auto-connect"
    "completion:Print shell completion script"
  )

  if (( CURRENT == 2 )); then
    _describe "command" commands
    return
  fi

  case "$words[2]" in
    connect)
      local -a connections
      connections=("${{(@f)$(oc __complete_connections "$words[CURRENT]")}}")
      _describe "connection" connections
      ;;
    completion)
      _values "shell" "bash" "zsh"
      ;;
    *)
      _default
      ;;
  esac
}}

compdef _oc oc
"#
    );
}

fn print_bash_completion() {
    println!(
        r#"_oc_complete() {{
  local cur prev cword
  COMPREPLY=()
  cur="${{COMP_WORDS[COMP_CWORD]}}"
  prev="${{COMP_WORDS[COMP_CWORD-1]}}"
  cword=$COMP_CWORD

  if [[ $cword -eq 1 ]]; then
    COMPREPLY=( $(compgen -W "ls connect completion" -- "$cur") )
    return
  fi

  case "${{COMP_WORDS[1]}}" in
    connect)
      COMPREPLY=( $(compgen -W "$(oc __complete_connections "$cur")" -- "$cur") )
      ;;
    completion)
      COMPREPLY=( $(compgen -W "bash zsh" -- "$cur") )
      ;;
  esac
}}

complete -F _oc_complete oc
"#
    );
}
