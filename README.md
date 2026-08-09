# cxq — Codex History Query

`cxq` is a read-only CLI for querying local Codex session transcripts. It scans
rollout JSONL under `$CODEX_HOME/sessions` and `$CODEX_HOME/archived_sessions`
without writing indexes, caches, or lock files.

Codex's transcript schema is not a stable public interface. `cxq` deliberately
uses tolerant parsing: unknown events are ignored and malformed lines are
reported without aborting the rest of a scan.

## Install

```sh
cargo install --path .
```

Or run `./install.sh` from a checkout to install into `${CARGO_HOME:-~/.cargo}/bin`.

On Windows, install Rust first and run the PowerShell installer:

```powershell
winget install Rustlang.Rustup
# Restart PowerShell after the Rust install, then:
.\install.ps1
```

The installer builds `cxq.exe`, installs it into `%CARGO_HOME%\bin` (or
`%USERPROFILE%\.cargo\bin`), and adds that directory to the persistent user
PATH when needed.

## Commands

| Command | Result |
|---|---|
| `cxq projects` | projects, session counts, corpus size, and activity range |
| `cxq sessions` | session metadata plus visible message and tool counts |
| `cxq prompts` | genuine `user_message` events |
| `cxq bash` | commands from direct and orchestrated exec calls |
| `cxq writes` | paths targeted by explicit patch/write tools |
| `cxq tools` | tool-use frequency by project and session |
| `cxq grep <regex>` | pointers into visible messages, tool calls, and results |
| `cxq show <session-prefix>` | safe transcript view for one session |

For Codex/ChatGPT app sessions, `cxq` expands the app's outer `exec` wrapper
when it can statically identify nested `tools.<name>(...)` calls. This makes
`bash`, `writes`, session tool totals, and `tools` report app activity alongside
direct CLI tool calls. Dynamic JavaScript dispatch remains visible through
`grep` and `show`, but is not guessed into a tool identity.

Common filters can appear before or after the subcommand:

```sh
# What did I ask in one session?
cxq prompts -s 019f9ad8

# Shell commands in this project during the last week
cxq bash -p cxq --since 7d

# Find sessions that touched Cargo.toml
cxq grep 'Cargo\.toml' --in tool-use -f tsv

# Inspect one physical transcript line and its neighborhood
cxq show 019f9ad8 --line 42 --around 5

# Query a different Codex home
cxq sessions --root /path/to/codex-home
```

Output formats are `table`, `tsv`, `json`, and `jsonl`. Table output truncates
long fields at 300 characters unless `--full` is used; machine formats preserve
full values. Run `cxq --help` or `cxq <command> --help` for the complete CLI.

## Privacy boundary

Session transcripts can contain source code, commands, tool results, prompts,
and other sensitive local data. Review output before sharing it.

`prompts`, `grep`, and `show` do not expose reasoning items, encrypted reasoning
payloads, base instructions, or developer-message context. Tool inputs and tool
results remain queryable because they are the purpose of the forensic commands.

## Scope

`cxq` covers history stored locally in the selected Codex home, including CLI,
app, IDE, exec, subagent, active, and archived rollout files that exist there.
It is not an account-wide ChatGPT or Codex Cloud history API.
