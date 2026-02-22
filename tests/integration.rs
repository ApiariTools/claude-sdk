//! Integration tests for the Claude SDK.
//!
//! These tests require `claude` CLI to be installed and authenticated.
//! Run with:
//!
//! ```sh
//! cargo test -p apiari-claude-sdk --test integration -- --ignored --nocapture
//! ```

use apiari_claude_sdk::{ClaudeClient, Event, SessionOptions};

/// Raw protocol capture test.
///
/// This test spawns `claude` directly using `tokio::process::Command` to capture
/// the raw NDJSON output, helping us verify our types match the real protocol.
#[tokio::test]
#[ignore]
async fn raw_protocol_capture() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::process::Command;

    let mut child = Command::new("claude")
        .args([
            "--print",
            "--output-format",
            "stream-json",
            "--input-format",
            "stream-json",
            "--verbose",
        ])
        // Clear CLAUDECODE env var to allow nested sessions (e.g. running
        // inside a Claude Code agent session).
        .env_remove("CLAUDECODE")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn claude CLI — is it installed?");

    // Send a simple message then close stdin to signal we're done.
    let mut stdin = child.stdin.take().unwrap();
    let input = serde_json::json!({
        "type": "user",
        "message": {
            "role": "user",
            "content": "Say hello in exactly 3 words. Nothing else."
        }
    });
    let input_line = serde_json::to_string(&input).unwrap();
    eprintln!(">>> SENDING: {input_line}");
    stdin.write_all(input_line.as_bytes()).await.unwrap();
    stdin.write_all(b"\n").await.unwrap();
    stdin.flush().await.unwrap();
    // Close stdin so claude knows there's no more input.
    drop(stdin);

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    let mut line_num = 0;
    let mut all_parsed = true;

    eprintln!("\n=== RAW PROTOCOL OUTPUT ===\n");

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await.unwrap();
        if n == 0 {
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        line_num += 1;
        eprintln!("LINE {line_num}: {trimmed}");

        // Try to parse and pretty-print the type field.
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(ty) = val.get("type").and_then(|t| t.as_str()) {
                eprintln!("  -> type = {ty:?}");
            }

            // Try to parse as our Message enum.
            match serde_json::from_value::<apiari_claude_sdk::Message>(val.clone()) {
                Ok(msg) => {
                    eprintln!("  -> PARSED OK: {msg:?}");
                }
                Err(e) => {
                    eprintln!("  -> PARSE FAILED: {e}");
                    eprintln!(
                        "  -> Raw JSON: {}",
                        serde_json::to_string_pretty(&val).unwrap()
                    );
                    all_parsed = false;
                }
            }
        }
        eprintln!();
    }

    // Also capture stderr for debugging.
    let stderr = child.stderr.take().unwrap();
    let mut stderr_reader = BufReader::new(stderr);
    let mut stderr_line = String::new();
    eprintln!("\n=== STDERR OUTPUT ===\n");
    loop {
        stderr_line.clear();
        let n = stderr_reader.read_line(&mut stderr_line).await.unwrap();
        if n == 0 {
            break;
        }
        eprintln!("STDERR: {}", stderr_line.trim());
    }

    let status = child.wait().await.unwrap();
    eprintln!("\n=== PROCESS EXITED: {status} ===");
    assert!(
        line_num > 0,
        "expected at least one line of output from claude (exit: {status})"
    );
    assert!(
        all_parsed,
        "some lines failed to parse as Message — see output above"
    );
}

/// SDK integration test.
///
/// This test uses the `ClaudeClient` to spawn a session, send a message,
/// and read events until completion.
#[tokio::test]
#[ignore]
async fn sdk_round_trip() {
    let client = ClaudeClient::new();

    let opts = SessionOptions {
        no_session_persistence: true,
        ..Default::default()
    };

    let mut session = client
        .spawn(opts)
        .await
        .expect("failed to spawn claude session");

    // Send a simple message.
    session
        .send_message("Say hello in exactly 3 words. Nothing else.")
        .await
        .expect("failed to send message");

    // Close stdin so the CLI knows there are no more messages coming.
    // This causes it to process the message and produce output.
    session.close_stdin();

    let mut got_system = false;
    let mut got_assistant = false;
    let mut got_result = false;
    let mut result_text = String::new();

    // Use a timeout to avoid hanging forever.
    let timeout = tokio::time::timeout(std::time::Duration::from_secs(120), async {
        loop {
            match session.next_event().await {
                Ok(Some(event)) => {
                    match &event {
                        Event::System(sys) => {
                            eprintln!("  -> Got system message (subtype={})", sys.subtype);
                            got_system = true;
                        }
                        Event::User(_) => {
                            eprintln!("  -> Got user echo");
                        }
                        Event::Assistant { message, .. } => {
                            eprintln!(
                                "  -> Got assistant message (model={})",
                                message.message.model
                            );
                            got_assistant = true;
                            for block in &message.message.content {
                                if let apiari_claude_sdk::ContentBlock::Text { text } = block {
                                    eprintln!("  -> Text: {text}");
                                    result_text.push_str(text);
                                }
                            }
                        }
                        Event::Result(result) => {
                            eprintln!("  -> Got result: subtype={}", result.subtype);
                            got_result = true;
                            break;
                        }
                        Event::RateLimit(_) => {
                            eprintln!("  -> Got rate limit event");
                        }
                        Event::Stream { .. } => {
                            eprintln!("  -> Got stream event");
                        }
                    }
                }
                Ok(None) => {
                    eprintln!("  -> EOF (session ended)");
                    break;
                }
                Err(e) => {
                    eprintln!("  -> ERROR: {e}");
                    panic!("Error reading event: {e}");
                }
            }
        }
    });

    match timeout.await {
        Ok(()) => {}
        Err(_) => panic!("Test timed out after 120 seconds"),
    }

    eprintln!("\n=== TEST RESULTS ===");
    eprintln!("Got system message: {got_system}");
    eprintln!("Got assistant message: {got_assistant}");
    eprintln!("Got result message: {got_result}");
    eprintln!("Response text: {result_text:?}");

    assert!(got_system, "Expected a system init message");
    assert!(got_assistant, "Expected an assistant message");
    assert!(got_result, "Expected a result message");
    assert!(
        !result_text.is_empty(),
        "Expected non-empty response text from the assistant"
    );
}
