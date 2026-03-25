# apiari-claude-sdk

Rust SDK wrapping the Claude Code CLI via NDJSON stdin/stdout.

## Quick Reference

```bash
cargo test -p apiari-claude-sdk           # Unit tests (6 + 3 doctests)
cargo test -p apiari-claude-sdk -- --ignored  # Integration tests (requires live `claude` CLI)
```

## Swarm Worker Rules

1. **You are working in a git worktree.** Always create a new branch (`swarm/*`), never commit directly to `main`.
2. **Only modify files within this repo (`claude-sdk/`).** Do not touch other repos in the workspace (e.g., `hive/`, `common/`, `swarm/`).
3. **When done, create a PR:**
   ```bash
   gh pr create --repo ApiariTools/apiari-claude-sdk --title "..." --body "..." --reviewer @copilot
   ```
4. **Do not run `cargo install` or modify system state.** No global installs, no modifying dotfiles, no system-level changes.

## Git Workflow

- You are working in a swarm worktree on a `swarm/*` branch. Stay on this branch.
- NEVER push to or merge into `main` directly.
- NEVER run `git push origin main` or `git checkout main`.
- When done, push your branch and open a PR. Swarm will handle merging.

## Architecture

```
src/
  lib.rs          # Module declarations + re-exports
  client.rs       # ClaudeClient (factory) + Session (live handle) + Event enum
  session.rs      # SessionOptions (25+ fields) + PermissionMode + to_cli_args()
  transport.rs    # NDJSON subprocess I/O (spawn, send, recv, kill, interrupt)
  types.rs        # All Claude CLI stream-json message types
  streaming.rs    # StreamAssembler (partial event -> complete blocks)
  tools.rs        # ToolUse + ToolResult convenience types
  error.rs        # SdkError enum + Result alias
tests/
  integration.rs  # Live CLI tests (#[ignore] by default)
```

## Protocol

Spawns: `claude --print --output-format stream-json --input-format stream-json --verbose [opts...]`

**Important**: Transport removes the `CLAUDECODE` env var before spawning to avoid nested session blocking.

### Message Flow

1. SDK writes `InputMessage` as NDJSON to subprocess stdin
2. Subprocess writes `Message` variants as NDJSON to stdout
3. SDK reads and dispatches as `Event` variants

### Message Types (stdin -> claude)

- `user` with text content
- `tool_result` with tool_use_id, output, is_error

### Message Types (claude -> stdout)

- `system` — session metadata (session_id, tools, model)
- `user` — echo of user turns
- `assistant` — model response with content blocks (text, thinking, tool_use, tool_result)
- `result` — final message (session complete, includes cost/duration)
- `rate_limit_event` — rate limit status

### Key Type Details

- `AssistantMessage` has a nested `message: AssistantMessageContent` (not flat)
- `ContentBlock` is `#[serde(tag = "type")]` with variants: text, thinking, tool_use, tool_result
- `Message` is `#[serde(tag = "type")]` with snake_case renaming

## Design Rules

- **Wrap CLI, not API.** This SDK spawns the `claude` binary. It does NOT call the Anthropic API directly. The CLI handles auth, tool execution, file access, and permissions.
- **Forward-compatible parsing.** Unknown message types are logged and skipped (not errors). Fields use `#[serde(default)]` liberally.
- **Async throughout.** All I/O uses tokio. `Transport` runs a background task to drain stderr.
- **No apiari-common dependency.** This crate is standalone.

## Integration Map

| Crate | How it uses claude-sdk |
|-------|----------------------|
| hive | Coordinator spawns sessions for `chat` and `plan` commands. Falls back to offline mode if CLI unavailable. |
| buzz | Does not use (polls external APIs directly) |
| swarm | Does not use (launches `claude` CLI directly as daemon subprocess) |
| keeper | Does not use (read-only dashboard) |

## Error Handling

`SdkError` variants:
- `ProcessSpawn` — claude binary not found or failed to start
- `ProcessDied { exit_code, stderr }` — subprocess exited unexpectedly
- `InvalidJson` — NDJSON parse failure
- `ProtocolError` — unexpected protocol state
- `Timeout` — operation timed out
- `Io` — underlying I/O error
- `NotConnected` — session already finished
