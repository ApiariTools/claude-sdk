# apiari-claude-sdk

Rust SDK for the [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code). Wraps the `claude` binary, communicating via newline-delimited JSON (NDJSON) over stdin/stdout.

This is **not** a direct API client. It spawns the `claude` CLI as a subprocess and uses its `--input-format stream-json --output-format stream-json` protocol. The CLI handles authentication, tool execution, file access, and permissions.

## Quick Start

```rust
use apiari_claude_sdk::{ClaudeClient, SessionOptions};

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
            apiari_claude_sdk::Event::Assistant { message, .. } => {
                for block in &message.message.content {
                    if let apiari_claude_sdk::ContentBlock::Text { text } = block {
                        print!("{text}");
                    }
                }
            }
            apiari_claude_sdk::Event::Result(result) => {
                println!("\nSession: {}", result.session_id);
                break;
            }
            _ => {}
        }
    }
    Ok(())
}
```

## Features

- Spawn and manage Claude CLI sessions
- Send messages and receive structured events
- Tool use/result protocol for custom tool handling
- Session resume, continue, and fork
- Stream assembler for partial message events
- Interrupt (SIGINT) support
- Full `SessionOptions` covering all CLI flags

## Requirements

- The `claude` CLI must be installed and on `$PATH` (or provide a custom path via `ClaudeClient::with_cli_path`)

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
apiari-claude-sdk.workspace = true
```
