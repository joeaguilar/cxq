use crate::model::{self, ToolCall};
use crate::out::Table;
use crate::scan::{self, Scope, SessionFile};
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

#[derive(Debug, Default)]
struct Summary {
    start: String,
    end: String,
    user_messages: usize,
    assistant_messages: usize,
    tools: usize,
}

pub fn projects(scope: &Scope, sessions: &[SessionFile]) -> Table {
    let summaries = summarize_all(scope, sessions);
    let mut grouped: BTreeMap<String, (usize, u64, String, String, usize)> = BTreeMap::new();
    for (session, summary) in sessions.iter().zip(summaries) {
        let entry = grouped.entry(session.project.clone()).or_insert((
            0,
            0,
            String::new(),
            String::new(),
            0,
        ));
        entry.0 += 1;
        entry.1 += session.size;
        entry.2 = minimum(&entry.2, &summary.start);
        entry.3 = maximum(&entry.3, &summary.end);
        entry.4 += usize::from(session.archived);
    }
    let mut table = Table::new(vec![
        "project", "sessions", "size", "first", "last", "archived",
    ]);
    table.rows = grouped
        .into_iter()
        .map(|(project, (count, bytes, first, last, archived))| {
            vec![
                project,
                count.to_string(),
                scan::human_size(bytes),
                first,
                last,
                archived.to_string(),
            ]
        })
        .collect();
    table
}

pub fn sessions(scope: &Scope, files: &[SessionFile]) -> Table {
    let summaries = summarize_all(scope, files);
    let mut table = Table::new(vec![
        "session",
        "project",
        "start",
        "end",
        "user_msgs",
        "asst_msgs",
        "tools",
        "source",
        "version",
        "cwd",
    ]);
    table.rows = files
        .iter()
        .zip(summaries)
        .filter(|(_, summary)| {
            summary.user_messages > 0 || summary.assistant_messages > 0 || summary.tools > 0
        })
        .map(|(session, summary)| {
            vec![
                session.id.clone(),
                session.project.clone(),
                summary.start,
                summary.end,
                summary.user_messages.to_string(),
                summary.assistant_messages.to_string(),
                summary.tools.to_string(),
                if session.archived {
                    "archived".to_owned()
                } else {
                    "active".to_owned()
                },
                session.cli_version.clone(),
                session.cwd.clone(),
            ]
        })
        .collect();
    table.rows.sort_by(|a, b| b[3].cmp(&a[3]));
    table
}

fn summarize_all(scope: &Scope, files: &[SessionFile]) -> Vec<Summary> {
    files
        .iter()
        .map(|session| summarize(scope, session))
        .collect()
}

fn summarize(scope: &Scope, session: &SessionFile) -> Summary {
    let mut summary = Summary::default();
    let result = scan::for_each_event(session, scope, |_, value| {
        if let Some(ts) = model::timestamp(value) {
            summary.start = minimum(&summary.start, ts);
            summary.end = maximum(&summary.end, ts);
        }
        if model::event_message(value, "user_message").is_some() {
            summary.user_messages += 1;
        }
        if model::assistant_message(value).is_some() {
            summary.assistant_messages += 1;
        }
        if model::tool_call(value).is_some() {
            summary.tools += 1;
        }
    });
    warn_read(session, result);
    summary
}

pub fn prompts(scope: &Scope, files: &[SessionFile], pattern: Option<&Regex>) -> Table {
    let mut table = Table::new(vec!["ts", "session", "project", "prompt"]);
    table.rows = scan::map(files, |session| {
        let mut rows = Vec::new();
        let result = scan::for_each_event(session, scope, |_, value| {
            let Some(prompt) = model::event_message(value, "user_message") else {
                return;
            };
            if pattern.is_none_or(|pattern| pattern.is_match(prompt)) {
                rows.push(vec![
                    model::timestamp(value).unwrap_or("").to_owned(),
                    short_id(&session.id),
                    session.project.clone(),
                    prompt.to_owned(),
                ]);
            }
        });
        warn_read(session, result);
        rows
    });
    table.rows.sort_by(|a, b| a[0].cmp(&b[0]));
    table
}

pub fn bash(scope: &Scope, files: &[SessionFile], pattern: Option<&Regex>) -> Table {
    let mut table = Table::new(vec![
        "ts",
        "session",
        "project",
        "tool",
        "description",
        "command",
    ]);
    table.rows = scan::map(files, |session| {
        let mut rows = Vec::new();
        let result = scan::for_each_event(session, scope, |_, value| {
            let Some(call) = model::tool_call(value) else {
                return;
            };
            let description = string_field(&call.input, "justification")
                .or_else(|| string_field(&call.input, "description"))
                .unwrap_or("");
            for command in shell_commands(&call) {
                if pattern.is_none_or(|pattern| pattern.is_match(&command)) {
                    rows.push(vec![
                        model::timestamp(value).unwrap_or("").to_owned(),
                        short_id(&session.id),
                        session.project.clone(),
                        call.name.clone(),
                        description.to_owned(),
                        command,
                    ]);
                }
            }
        });
        warn_read(session, result);
        rows
    });
    table.rows.sort_by(|a, b| a[0].cmp(&b[0]));
    table
}

pub fn writes(scope: &Scope, files: &[SessionFile], pattern: Option<&Regex>) -> Table {
    let mut table = Table::new(vec!["ts", "session", "project", "tool", "file_path"]);
    table.rows = scan::map(files, |session| {
        let mut rows = Vec::new();
        let result = scan::for_each_event(session, scope, |_, value| {
            let Some(call) = model::tool_call(value) else {
                return;
            };
            for path in written_paths(&call) {
                if pattern.is_none_or(|pattern| pattern.is_match(&path)) {
                    rows.push(vec![
                        model::timestamp(value).unwrap_or("").to_owned(),
                        short_id(&session.id),
                        session.project.clone(),
                        call.name.clone(),
                        path,
                    ]);
                }
            }
        });
        warn_read(session, result);
        rows
    });
    table.rows.sort_by(|a, b| a[0].cmp(&b[0]));
    table
}

pub fn tools(scope: &Scope, files: &[SessionFile]) -> Table {
    let triples = scan::map(files, |session| {
        let mut rows = Vec::new();
        let result = scan::for_each_event(session, scope, |_, value| {
            if let Some(call) = model::tool_call(value) {
                rows.push((call.name, session.project.clone(), session.id.clone()));
            }
        });
        warn_read(session, result);
        rows
    });
    let mut grouped: BTreeMap<String, (usize, BTreeSet<String>, BTreeSet<String>)> =
        BTreeMap::new();
    for (tool, project, session) in triples {
        let entry = grouped.entry(tool).or_default();
        entry.0 += 1;
        entry.1.insert(project);
        entry.2.insert(session);
    }
    let mut table = Table::new(vec!["tool", "count", "projects", "sessions"]);
    table.rows = grouped
        .into_iter()
        .map(|(tool, (count, projects, sessions))| {
            vec![
                tool,
                count.to_string(),
                projects.len().to_string(),
                sessions.len().to_string(),
            ]
        })
        .collect();
    table
        .rows
        .sort_by_key(|row| std::cmp::Reverse(row[1].parse::<usize>().unwrap_or(0)));
    table
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum GrepArea {
    Any,
    Text,
    ToolUse,
    ToolResult,
}

pub fn grep(scope: &Scope, files: &[SessionFile], pattern: &Regex, area: GrepArea) -> Table {
    let mut table = Table::new(vec![
        "session",
        "project",
        "line_no",
        "type",
        "match_context",
    ]);
    table.rows = scan::map(files, |session| {
        let mut rows = Vec::new();
        let result = scan::for_each_event(session, scope, |line_no, value| {
            let fields = searchable(value, area);
            for (kind, text) in fields {
                if let Some(found) = pattern.find(&text) {
                    rows.push(vec![
                        short_id(&session.id),
                        session.project.clone(),
                        line_no.to_string(),
                        kind.to_owned(),
                        context(&text, found.start(), found.end()),
                    ]);
                    break;
                }
            }
        });
        warn_read(session, result);
        rows
    });
    table
}

pub fn show(
    scope: &Scope,
    files: &[SessionFile],
    prefix: &str,
    line: Option<usize>,
    around: usize,
) -> Result<Table, String> {
    if prefix.len() < 4 {
        return Err("session prefix must contain at least 4 characters".to_owned());
    }
    let matches: Vec<_> = files
        .iter()
        .filter(|session| session.id.starts_with(prefix))
        .collect();
    let session = match matches.as_slice() {
        [] => return Err(format!("no session matching '{prefix}'")),
        [session] => *session,
        _ => return Err(format!("session prefix '{prefix}' is ambiguous")),
    };
    let mut table = Table::new(vec!["line_no", "ts", "type", "detail"]);
    let result = scan::for_each_event(session, scope, |line_no, value| {
        if line.is_some_and(|target| line_no.abs_diff(target) > around) {
            return;
        }
        if let Some(message) = model::event_message(value, "user_message") {
            table.rows.push(vec![
                line_no.to_string(),
                model::timestamp(value).unwrap_or("").to_owned(),
                "user".to_owned(),
                message.to_owned(),
            ]);
        } else if let Some(message) = model::assistant_message(value) {
            table.rows.push(vec![
                line_no.to_string(),
                model::timestamp(value).unwrap_or("").to_owned(),
                "assistant".to_owned(),
                message.to_owned(),
            ]);
        } else if let Some(call) = model::tool_call(value) {
            table.rows.push(vec![
                line_no.to_string(),
                model::timestamp(value).unwrap_or("").to_owned(),
                format!("tool:{}", call.name),
                call.input.to_string(),
            ]);
        } else if let Some((_, output)) = model::tool_output(value) {
            table.rows.push(vec![
                line_no.to_string(),
                model::timestamp(value).unwrap_or("").to_owned(),
                "tool_result".to_owned(),
                output,
            ]);
        }
    });
    warn_read(session, result);
    Ok(table)
}

fn searchable(value: &Value, area: GrepArea) -> Vec<(&'static str, String)> {
    let mut fields = Vec::new();
    if matches!(area, GrepArea::Any | GrepArea::Text) {
        if let Some(text) = model::event_message(value, "user_message") {
            fields.push(("user", text.to_owned()));
        }
        if let Some(text) = model::assistant_message(value) {
            fields.push(("assistant", text.to_owned()));
        }
    }
    if matches!(area, GrepArea::Any | GrepArea::ToolUse) {
        if let Some(call) = model::tool_call(value) {
            fields.push(("tool_use", format!("{} {}", call.name, call.input)));
        }
    }
    if matches!(area, GrepArea::Any | GrepArea::ToolResult) {
        if let Some((_, output)) = model::tool_output(value) {
            fields.push(("tool_result", output));
        }
    }
    fields
}

fn shell_commands(call: &ToolCall) -> Vec<String> {
    if matches!(
        call.name.as_str(),
        "exec_command" | "shell" | "shell_command"
    ) {
        return string_field(&call.input, "cmd")
            .or_else(|| string_field(&call.input, "command"))
            .map(|command| vec![command.to_owned()])
            .unwrap_or_default();
    }
    if call.name != "exec" {
        return Vec::new();
    }
    let Some(code) = call
        .input
        .as_str()
        .or_else(|| string_field(&call.input, "code"))
    else {
        return Vec::new();
    };
    nested_command_regex()
        .captures_iter(code)
        .filter_map(|captures| captures.name("quoted"))
        .filter_map(|quoted| serde_json::from_str::<String>(quoted.as_str()).ok())
        .collect()
}

fn nested_command_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r#"exec_command\s*\(\s*\{(?s:.*?)["']cmd["']\s*:\s*(?<quoted>"(?:\\.|[^"\\])*")"#,
        )
        .expect("valid command extraction regex")
    })
}

fn written_paths(call: &ToolCall) -> Vec<String> {
    if call.name.contains("apply_patch") {
        let patch = call
            .input
            .as_str()
            .or_else(|| string_field(&call.input, "input"))
            .or_else(|| string_field(&call.input, "patch"))
            .unwrap_or("");
        return patch
            .lines()
            .filter_map(|line| {
                ["*** Add File: ", "*** Update File: ", "*** Delete File: "]
                    .iter()
                    .find_map(|prefix| line.strip_prefix(prefix))
            })
            .map(str::to_owned)
            .collect();
    }
    if matches!(
        call.name.as_str(),
        "write_file" | "edit_file" | "create_file"
    ) {
        return ["path", "file_path"]
            .iter()
            .filter_map(|key| string_field(&call.input, key))
            .map(str::to_owned)
            .collect();
    }
    Vec::new()
}

fn string_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str()
}

fn short_id(id: &str) -> String {
    id.chars().take(12).collect()
}

fn minimum(current: &str, candidate: &str) -> String {
    if current.is_empty() || (!candidate.is_empty() && candidate < current) {
        candidate.to_owned()
    } else {
        current.to_owned()
    }
}

fn maximum(current: &str, candidate: &str) -> String {
    if candidate > current {
        candidate.to_owned()
    } else {
        current.to_owned()
    }
}

fn context(text: &str, start: usize, end: usize) -> String {
    let left = text[..start]
        .char_indices()
        .rev()
        .nth(80)
        .map_or(0, |(i, _)| i);
    let right = text[end..]
        .char_indices()
        .nth(120)
        .map_or(text.len(), |(i, _)| end + i);
    text[left..right].to_owned()
}

fn warn_read(session: &SessionFile, result: std::io::Result<()>) {
    if let Err(error) = result {
        eprintln!("cxq: skipped {}: {error}", session.path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_nested_exec_commands() {
        let call = ToolCall {
            name: "exec".to_owned(),
            input: serde_json::json!({
                "code": r#"const r = await tools.exec_command({"cmd":"cargo test","yield_time_ms":10000}); text(r.output);"#
            }),
        };
        assert_eq!(shell_commands(&call), vec!["cargo test"]);
    }

    #[test]
    fn extracts_nested_exec_commands_from_freeform_input() {
        let call = ToolCall {
            name: "exec".to_owned(),
            input: Value::String(
                r#"const r = await tools.exec_command({"cmd":"cargo clippy"}); text(r.output);"#
                    .to_owned(),
            ),
        };
        assert_eq!(shell_commands(&call), vec!["cargo clippy"]);
    }

    #[test]
    fn extracts_apply_patch_paths() {
        let call = ToolCall {
            name: "apply_patch".to_owned(),
            input: Value::String(
                "*** Begin Patch\n*** Add File: src/main.rs\n*** Update File: README.md\n"
                    .to_owned(),
            ),
        };
        assert_eq!(written_paths(&call), vec!["src/main.rs", "README.md"]);
    }

    #[test]
    fn reasoning_is_not_searchable() {
        let value = serde_json::json!({
            "type": "response_item",
            "payload": {"type": "reasoning", "summary": [{"text": "private"}]}
        });
        assert!(searchable(&value, GrepArea::Any).is_empty());
    }
}
