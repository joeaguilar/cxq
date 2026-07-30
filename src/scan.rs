use crate::{model, timearg};
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use serde_json::Value;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static MALFORMED: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone)]
pub struct Scope {
    pub codex_home: PathBuf,
    pub projects: Vec<String>,
    pub sessions: Vec<String>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct SessionFile {
    pub path: PathBuf,
    pub id: String,
    pub cwd: String,
    pub project: String,
    pub cli_version: String,
    pub archived: bool,
    pub size: u64,
}

pub fn discover(scope: &Scope) -> Vec<SessionFile> {
    let mut paths = Vec::new();
    collect_jsonl(&scope.codex_home.join("sessions"), false, &mut paths);
    collect_jsonl(
        &scope.codex_home.join("archived_sessions"),
        true,
        &mut paths,
    );

    let mut sessions: Vec<_> = paths
        .into_par_iter()
        .filter_map(|(path, archived)| inspect(path, archived))
        .filter(|session| {
            (scope.projects.is_empty()
                || scope.projects.iter().any(|pattern| {
                    session.project.contains(pattern) || session.cwd.contains(pattern)
                }))
                && (scope.sessions.is_empty()
                    || scope
                        .sessions
                        .iter()
                        .any(|prefix| session.id.starts_with(prefix)))
        })
        .collect();
    sessions.sort_by(|a, b| a.path.cmp(&b.path));
    sessions
}

fn collect_jsonl(root: &Path, archived: bool, output: &mut Vec<(PathBuf, bool)>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl(&path, archived, output);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "jsonl")
        {
            output.push((path, archived));
        }
    }
}

fn inspect(path: PathBuf, archived: bool) -> Option<SessionFile> {
    let size = path.metadata().ok()?.len();
    let file = File::open(&path).ok()?;
    let reader = BufReader::new(file);
    for line in reader.lines().take(20).flatten() {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some((id, cwd, cli_version)) = model::session_meta(&value) else {
            continue;
        };
        let project = Path::new(cwd)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(cwd)
            .to_owned();
        return Some(SessionFile {
            path,
            id: id.to_owned(),
            cwd: cwd.to_owned(),
            project,
            cli_version: cli_version.to_owned(),
            archived,
            size,
        });
    }
    None
}

pub fn for_each_event(
    session: &SessionFile,
    scope: &Scope,
    mut visitor: impl FnMut(usize, &Value),
) -> io::Result<()> {
    let file = File::open(&session.path)?;
    let reader = BufReader::with_capacity(1024 * 1024, file);
    for (index, line) in reader.lines().enumerate() {
        let line = line?;
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            MALFORMED.fetch_add(1, Ordering::Relaxed);
            continue;
        };
        if in_time_scope(&value, scope) {
            visitor(index + 1, &value);
        }
    }
    Ok(())
}

fn in_time_scope(value: &Value, scope: &Scope) -> bool {
    let Some(ts) = model::timestamp(value).and_then(timearg::event) else {
        return scope.since.is_none() && scope.until.is_none();
    };
    scope.since.is_none_or(|since| ts >= since) && scope.until.is_none_or(|until| ts <= until)
}

pub fn malformed_count() -> usize {
    MALFORMED.load(Ordering::Relaxed)
}

pub fn map<T: Send>(
    sessions: &[SessionFile],
    function: impl Fn(&SessionFile) -> Vec<T> + Sync + Send,
) -> Vec<T> {
    sessions.par_iter().flat_map(function).collect()
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
