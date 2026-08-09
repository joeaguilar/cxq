//! cxq — read-only queries over local Codex session history.

mod commands;
mod model;
mod out;
mod scan;
mod timearg;

use clap::{Args, Parser, Subcommand};
use out::Format;
use regex::Regex;
use scan::Scope;
use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(
    name = "cxq",
    version,
    about = "Query local Codex session history (read-only)",
    long_about = "Read-only, schema-tolerant queries over rollout JSONL stored under CODEX_HOME."
)]
struct Cli {
    #[command(flatten)]
    common: Common,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Args)]
struct Common {
    /// Codex home containing `sessions/` and `archived_sessions/`.
    #[arg(long, global = true, value_name = "DIR")]
    root: Option<PathBuf>,

    /// Match a project basename or recorded working directory. Repeatable.
    #[arg(short = 'p', long, global = true, value_name = "PATTERN")]
    project: Vec<String>,

    /// Match a session UUID prefix (at least four characters). Repeatable.
    #[arg(short = 's', long, global = true, value_name = "PREFIX")]
    session: Vec<String>,

    /// Include events at or after this time (RFC3339, date, or 7d/24h/90m).
    #[arg(long, global = true)]
    since: Option<String>,

    /// Include events at or before this time.
    #[arg(long, global = true)]
    until: Option<String>,

    /// Output format.
    #[arg(short = 'f', long, global = true, value_enum, default_value = "table")]
    format: Format,

    /// Do not truncate long values in table output.
    #[arg(long, global = true)]
    full: bool,

    /// Maximum output rows (table defaults to 200; machine formats are unlimited).
    #[arg(long, global = true)]
    limit: Option<usize>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List projects with session counts, size, and activity range.
    Projects,
    /// List sessions with message and tool counts.
    Sessions,
    /// List genuine user prompts (injected developer context is excluded).
    Prompts {
        /// Keep prompts matching this regular expression.
        #[arg(long)]
        grep: Option<String>,
    },
    /// List shell commands from direct and orchestrated exec tool calls.
    Bash {
        /// Keep commands matching this regular expression.
        #[arg(long)]
        grep: Option<String>,
    },
    /// List files targeted by explicit file-write and apply-patch tools.
    Writes {
        /// Keep paths matching this regular expression.
        #[arg(long)]
        grep: Option<String>,
    },
    /// Roll up tool-use counts across projects and sessions.
    Tools,
    /// Search visible messages, tool calls, and tool results (never reasoning).
    Grep {
        /// Regular expression to search for.
        pattern: String,
        /// Restrict the searched event area.
        #[arg(long = "in", value_enum, default_value = "any")]
        area: commands::GrepArea,
    },
    /// Pretty-print safe, visible events from one session.
    Show {
        /// Unique session UUID prefix.
        session_prefix: String,
        /// Show the event at this physical JSONL line.
        #[arg(long)]
        line: Option<usize>,
        /// Include this many physical lines around --line.
        #[arg(long, default_value_t = 3)]
        around: usize,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(error) => {
            eprintln!("cxq: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool, String> {
    let cli = Cli::parse();
    validate_prefixes(&cli.common.session)?;
    let codex_home = cli.common.root.clone().unwrap_or_else(default_codex_home);
    if !codex_home.join("sessions").is_dir() && !codex_home.join("archived_sessions").is_dir() {
        return Err(format!(
            "no sessions/ or archived_sessions/ under {}",
            codex_home.display()
        ));
    }
    let scope = Scope {
        codex_home,
        projects: cli.common.project.clone(),
        sessions: cli.common.session.clone(),
        since: cli
            .common
            .since
            .as_deref()
            .map(|value| timearg::parse(value, false))
            .transpose()?,
        until: cli
            .common
            .until
            .as_deref()
            .map(|value| timearg::parse(value, true))
            .transpose()?,
    };
    let files = scan::discover(&scope);
    let table = match &cli.command {
        Command::Projects => commands::projects(&scope, &files),
        Command::Sessions => commands::sessions(&scope, &files),
        Command::Prompts { grep } => {
            let pattern = compile_optional(grep.as_deref())?;
            commands::prompts(&scope, &files, pattern.as_ref())
        }
        Command::Bash { grep } => {
            let pattern = compile_optional(grep.as_deref())?;
            commands::bash(&scope, &files, pattern.as_ref())
        }
        Command::Writes { grep } => {
            let pattern = compile_optional(grep.as_deref())?;
            commands::writes(&scope, &files, pattern.as_ref())
        }
        Command::Tools => commands::tools(&scope, &files),
        Command::Grep { pattern, area } => {
            let pattern = Regex::new(pattern)
                .map_err(|error| format!("invalid regular expression: {error}"))?;
            commands::grep(&scope, &files, &pattern, *area)
        }
        Command::Show {
            session_prefix,
            line,
            around,
        } => commands::show(&scope, &files, session_prefix, *line, *around)?,
    };
    let default_limit = if matches!(cli.common.format, Format::Table) {
        Some(200)
    } else {
        None
    };
    let rendered = out::render(
        table,
        cli.common.format,
        cli.common.full,
        cli.common.limit.or(default_limit),
    )
    .map_err(|error| format!("writing output: {error}"))?;
    let malformed = scan::malformed_count();
    if malformed > 0 {
        eprintln!("cxq: skipped {malformed} malformed JSONL lines");
    }
    Ok(rendered > 0)
}

fn validate_prefixes(prefixes: &[String]) -> Result<(), String> {
    if let Some(prefix) = prefixes.iter().find(|prefix| prefix.len() < 4) {
        return Err(format!(
            "session prefix '{prefix}' is too short (need at least 4 characters)"
        ));
    }
    Ok(())
}

fn compile_optional(pattern: Option<&str>) -> Result<Option<Regex>, String> {
    pattern
        .map(|pattern| {
            Regex::new(pattern).map_err(|error| format!("invalid regular expression: {error}"))
        })
        .transpose()
}

fn default_codex_home() -> PathBuf {
    codex_home_from_env(
        env::var_os("CODEX_HOME"),
        env::var_os("HOME"),
        env::var_os("USERPROFILE"),
    )
}

fn codex_home_from_env(
    codex_home: Option<OsString>,
    home: Option<OsString>,
    user_profile: Option<OsString>,
) -> PathBuf {
    codex_home.map_or_else(
        || {
            home.or(user_profile).map_or_else(
                || PathBuf::from(".codex"),
                |home| PathBuf::from(home).join(".codex"),
            )
        },
        PathBuf::from,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_home_override_wins() {
        assert_eq!(
            codex_home_from_env(
                Some(OsString::from("custom")),
                Some(OsString::from("home")),
                Some(OsString::from("profile")),
            ),
            PathBuf::from("custom")
        );
    }

    #[test]
    fn user_profile_is_the_windows_home_fallback() {
        assert_eq!(
            codex_home_from_env(None, None, Some(OsString::from("profile"))),
            PathBuf::from("profile").join(".codex")
        );
    }
}
