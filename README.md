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

The `claude` CLI must be installed and on `$PATH`, or provide a custom path via `ClaudeClient::with_cli_path`.
