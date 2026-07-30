# cxq v1 contract

## Purpose

Provide fast, read-only answers over locally persisted Codex rollout transcripts
without hand-written `jq`, SQLite coupling, or a background index.

## Guarantees

- Read only `$CODEX_HOME/sessions` and `$CODEX_HOME/archived_sessions`.
- Stream each JSONL file; memory is proportional to result size plus the longest
  line, not corpus size.
- Scan files in parallel while processing each file in order.
- Ignore unknown event and payload fields.
- Skip malformed JSONL lines and report the count on stderr.
- Never expose reasoning, encrypted reasoning, base instructions, or developer
  context through the safe text commands.
- Exit `0` when rows match, `1` when no rows match, and `2` for usage or I/O
  errors.

## Sources and normalization

The first `session_meta` record supplies the session UUID, recorded working
directory, and producing CLI version.

Visible human and assistant text comes only from `event_msg` records with
payload types `user_message` and `agent_message`. This avoids treating injected
`response_item` messages as human prompts.

Tool calls come from `response_item` payload types `function_call` and
`custom_tool_call`. JSON-encoded argument strings are normalized into JSON
values. Direct `exec_command` calls expose `cmd`; newer orchestrated `exec`
calls are inspected for nested `tools.exec_command({"cmd": ...})` calls.

Explicit patch and write tools drive `writes`. Shell redirections inside a
command are intentionally not inferred as file writes.

## Non-goals

- No cache, index, database, daemon, or watch mode.
- No dependency on Codex's internal SQLite schema.
- No cloud/account-wide history.
- No mutation, resume, archive, delete, or replay operations.
- No agent-tree forensics in v1.
- No promise that OpenAI's private rollout schema will remain unchanged.

## Compatibility strategy

The transcript path is documented by OpenAI, but the transcript format is not a
stable interface. Fixtures therefore represent observed semantic shapes rather
than exhaustive schemas. New event variants should degrade to omitted fields,
not failed scans. `cli_version` is surfaced by `sessions` to make regressions
diagnosable.

