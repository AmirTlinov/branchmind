#![forbid(unsafe_code)]

use crate::{JsonRpcRequest, McpServer, json_rpc_error};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StdioMode {
    NewlineJson,
    ContentLength,
}

fn detect_mode_from_first_line(line: &str) -> Option<StdioMode> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Some(StdioMode::NewlineJson);
    }

    // MCP spec framing: Content-Length headers followed by a blank line and a JSON body.
    // Some clients may send Content-Type first; treat any plausible header line as header mode.
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("content-length:") || lower.starts_with("content-type:") {
        return Some(StdioMode::ContentLength);
    }

    None
}

fn parse_content_length_header(line: &str) -> Option<usize> {
    let trimmed = line.trim();
    let (key, value) = trimmed.split_once(':')?;
    if !key.trim().eq_ignore_ascii_case("content-length") {
        return None;
    }
    value.trim().parse::<usize>().ok()
}

fn read_content_length_frame(
    reader: &mut BufReader<std::io::StdinLock<'_>>,
    mut first_header: String,
) -> std::io::Result<Option<Vec<u8>>> {
    const MAX_CONTENT_LENGTH_BYTES: usize = 16 * 1024 * 1024;

    let mut content_length: Option<usize> = parse_content_length_header(&first_header);

    loop {
        let trimmed = first_header.trim_end();
        if trimmed.is_empty() {
            break;
        }

        first_header.clear();
        let read = reader.read_line(&mut first_header)?;
        if read == 0 {
            // EOF mid-header: treat as connection close.
            return Ok(None);
        }

        if content_length.is_none() {
            content_length = parse_content_length_header(&first_header);
        }
    }

    let Some(len) = content_length else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Missing Content-Length header",
        ));
    };
    if len > MAX_CONTENT_LENGTH_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Content-Length exceeds max allowed size",
        ));
    }

    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;
    Ok(Some(body))
}

fn write_newline_json(
    stdout: &mut std::io::StdoutLock<'_>,
    resp: &Value,
) -> Result<(), Box<dyn std::error::Error>> {
    writeln!(stdout, "{}", serde_json::to_string(resp)?)?;
    stdout.flush()?;
    Ok(())
}

fn write_content_length_json(
    stdout: &mut std::io::StdoutLock<'_>,
    resp: &Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let body = serde_json::to_vec(resp)?;
    write!(stdout, "Content-Length: {}\r\n\r\n", body.len())?;
    stdout.write_all(&body)?;
    stdout.flush()?;
    Ok(())
}

pub(crate) fn run_stdio(server: &mut McpServer) -> Result<(), Box<dyn std::error::Error>> {
    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let mut stdout = std::io::stdout().lock();

    // Auto-detect framing once per process. This keeps responses consistent and avoids
    // interleaving different framing styles on the same transport.
    let mut mode: Option<StdioMode> = None;

    loop {
        let effective_mode = match mode {
            Some(v) => v,
            None => {
                let mut peek = String::new();
                let read = reader.read_line(&mut peek)?;
                if read == 0 {
                    break;
                }
                if let Some(detected) = detect_mode_from_first_line(&peek) {
                    mode = Some(detected);
                }
                // Re-process the line now that we have a mode (or skip empties).
                if mode.is_none() {
                    continue;
                }

                let detected = mode.unwrap();
                match detected {
                    StdioMode::NewlineJson => {
                        let raw = peek.trim();
                        if raw.is_empty() {
                            continue;
                        }
                        handle_newline_request(server, &mut stdout, raw)?;
                        continue;
                    }
                    StdioMode::ContentLength => {
                        let Some(body) = read_content_length_frame(&mut reader, peek)? else {
                            break;
                        };
                        handle_content_length_request(server, &mut stdout, &body)?;
                        continue;
                    }
                }
            }
        };

        match effective_mode {
            StdioMode::NewlineJson => {
                let mut line = String::new();
                let read = reader.read_line(&mut line)?;
                if read == 0 {
                    break;
                }
                let raw = line.trim();
                if raw.is_empty() {
                    continue;
                }
                handle_newline_request(server, &mut stdout, raw)?;
            }
            StdioMode::ContentLength => {
                let mut first_header = String::new();
                let read = reader.read_line(&mut first_header)?;
                if read == 0 {
                    break;
                }
                if first_header.trim().is_empty() {
                    continue;
                }
                let Some(body) = read_content_length_frame(&mut reader, first_header)? else {
                    break;
                };
                handle_content_length_request(server, &mut stdout, &body)?;
            }
        }
    }

    Ok(())
}

fn handle_newline_request(
    server: &mut McpServer,
    stdout: &mut std::io::StdoutLock<'_>,
    raw: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let parsed: Result<Value, _> = serde_json::from_str(raw);
    let data = match parsed {
        Ok(v) => v,
        Err(e) => {
            let resp = json_rpc_error(None, -32700, &format!("Parse error: {e}"));
            write_newline_json(stdout, &resp)?;
            return Ok(());
        }
    };

    let (id, has_method) = match data.as_object() {
        Some(obj) => (obj.get("id").cloned(), obj.contains_key("method")),
        None => {
            let resp = json_rpc_error(None, -32600, "Invalid Request");
            write_newline_json(stdout, &resp)?;
            return Ok(());
        }
    };
    if !has_method {
        let resp = json_rpc_error(id, -32600, "Invalid Request");
        write_newline_json(stdout, &resp)?;
        return Ok(());
    }

    let request: JsonRpcRequest = match serde_json::from_value(data) {
        Ok(v) => v,
        Err(e) => {
            let resp = json_rpc_error(id, -32600, &format!("Invalid Request: {e}"));
            write_newline_json(stdout, &resp)?;
            return Ok(());
        }
    };

    if let Some(resp) = server.handle(request) {
        write_newline_json(stdout, &resp)?;
    }

    Ok(())
}

fn handle_content_length_request(
    server: &mut McpServer,
    stdout: &mut std::io::StdoutLock<'_>,
    body: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let data: Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => {
            let resp = json_rpc_error(None, -32700, &format!("Parse error: {e}"));
            write_content_length_json(stdout, &resp)?;
            return Ok(());
        }
    };

    let (id, has_method) = match data.as_object() {
        Some(obj) => (obj.get("id").cloned(), obj.contains_key("method")),
        None => {
            let resp = json_rpc_error(None, -32600, "Invalid Request");
            write_content_length_json(stdout, &resp)?;
            return Ok(());
        }
    };
    if !has_method {
        let resp = json_rpc_error(id, -32600, "Invalid Request");
        write_content_length_json(stdout, &resp)?;
        return Ok(());
    }

    let request: JsonRpcRequest = match serde_json::from_value(data) {
        Ok(v) => v,
        Err(e) => {
            let resp = json_rpc_error(id, -32600, &format!("Invalid Request: {e}"));
            write_content_length_json(stdout, &resp)?;
            return Ok(());
        }
    };

    if let Some(resp) = server.handle(request) {
        write_content_length_json(stdout, &resp)?;
    }

    Ok(())
}
