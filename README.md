# apiari-claude-sdk

Rust SDK for the [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code). Spawns the `claude` binary as a subprocess and communicates via newline-delimited JSON (NDJSON) over stdin/stdout.

This is **not** a direct API client — the CLI handles authentication, tool execution, and permissions. The SDK provides typed Rust bindings over the streaming protocol.

## How It Works

The SDK spawns `claude` with the streaming JSON protocol:

```
claude --print --output-format stream-json --input-format stream-json --verbose
```

It writes `InputMessage` objects (user text or tool results) to stdin and reads structured events from stdout. The `CLAUDECODE` env var is removed from the subprocess to prevent nested session blocking.

**Key types:** `ClaudeClient` (session factory), `Session` (live handle), `Event` (system/assistant/result/stream), `ToolUse` / `ToolResult` (tool protocol), `SessionOptions` (all CLI flags), `StreamAssembler` (partial message reassembly).

## Quick Start

```rust
use apiari_claude_sdk::{ClaudeClient, Event, ContentBlock, SessionOptions};

#[tokio::main]
async fn main() -> apiari_claude_sdk::Result<()> {
    let client = ClaudeClient::new();
    let mut session = client.spawn(SessionOptions {
        model: Some("sonnet".into()),
        allowed_tools: vec!["Bash".into(), "Read".into()],
        ..Default::default()
    }).await?;

    session.send_message("List files in the current directory").await?;

    while let Some(event) = session.next_event().await? {
        match event {
            Event::Assistant { message, tool_uses } => {
                for block in &message.message.content {
                    if let ContentBlock::Text { text } = block {
                        print!("{text}");
                    }
                }
                for tool in &tool_uses {
                    println!("\nTool call: {} ({})", tool.name, tool.id);
                }
            }
            Event::Result(_) => break,
            _ => {}
        }
    }
    Ok(())
}
```

## Features

- Spawn and manage multi-turn Claude CLI sessions
- Typed event stream: `System`, `Assistant`, `Result`, `Stream`, `RateLimit`
- Tool use/result protocol for custom tool handling
- Session resume, continue, and fork
- Stream assembler for partial/incremental message events
- Interrupt (`SIGINT`) and graceful shutdown
- Full `SessionOptions` covering all CLI flags

## Requirements

- The `claude` CLI must be installed and on `$PATH` (or provide a custom path via `ClaudeClient::with_cli_path`)

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
apiari-claude-sdk.workspace = true
```

## Execution Model — Bidirectional Interactive

The Claude SDK uses a **bidirectional** protocol. A single `Session` stays alive for the full conversation, and you can send messages and tool results at any time:

```
┌─────────┐       stdin (NDJSON)        ┌───────────┐
│   SDK   │ ──── send_message() ──────► │  claude   │
│         │ ──── send_tool_result() ──► │  CLI      │
│         │                             │           │
│         │ ◄──── next_event() ──────── │  (stdout) │
└─────────┘       NDJSON stream         └───────────┘
```

### How a Conversation Works

```rust
// 1. Spawn a session — the subprocess stays alive
let mut session = client.spawn(opts).await?;

// 2. Send a message at any time
session.send_message("Do something").await?;

// 3. Read events — model may request tool execution
while let Some(event) = session.next_event().await? {
    match event {
        Event::Assistant { tool_uses, .. } => {
            // 4. Send tool results back mid-conversation
            for tu in &tool_uses {
                session.send_tool_result(&ToolResult {
                    tool_use_id: tu.id.clone(),
                    output: "tool output here".into(),
                    is_error: false,
                }).await?;
            }
        }
        Event::Result(_) => break,
        _ => {}
    }
}

// 5. Or send another message — same session, continues the conversation
session.send_message("Now do something else").await?;
```

### Key Properties

- **Session stays alive** across multiple messages — one subprocess for the whole conversation.
- **Input is always available** — you can `send_message()` or `send_tool_result()` at any point during the session.
- **Tool results are required** — when the model emits `tool_use` blocks, it pauses and waits for your `send_tool_result()` before continuing.
- **`interrupt()` pauses** the current operation, but the session remains open for further interaction.
- **`close_stdin()`** signals EOF — the model finishes its current work and the session ends.

### UI Implications

- **Input can stay enabled** throughout the session — messages can be sent at any time.
- **Events stream in real-time** via `next_event()` — render them as they arrive.
- **Tool execution is your responsibility** — the SDK gives you `ToolUse` requests, you execute them and send results back. (The CLI can also handle tools internally depending on `SessionOptions`.)

### Comparison with Codex SDK

| | Claude SDK | Codex SDK |
|---|---|---|
| **Protocol** | Bidirectional (stdin + stdout) | Unidirectional (stdout only) |
| **Session lifetime** | One subprocess for entire conversation | One subprocess per message |
| **Mid-turn input** | Yes (`send_message`, `send_tool_result`) | No (stdin is `/dev/null`) |
| **Tool execution** | SDK receives tool requests, sends results | CLI handles tools internally |
| **Multi-turn chat** | Send multiple messages on same session | Resume with session ID for each message |
| **Input availability** | Always enabled | Disabled during execution |

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

### Message Types

**SDK → CLI (stdin):**
- `user` — text message or tool result

**CLI → SDK (stdout):**
- `system` — session metadata (emitted once at start)
- `user` — echo of user turns
- `assistant` — model response with content blocks (text, thinking, tool_use, tool_result)
- `result` — final summary (session complete, includes cost/duration/session_id)
- `stream_event` — raw API streaming events (when `include_partial_messages` is set)
- `rate_limit_event` — rate limit status
