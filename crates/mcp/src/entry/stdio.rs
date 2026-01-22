#![forbid(unsafe_code)]

use crate::McpServer;
use crate::entry::framing::{
    TransportMode, detect_mode_from_first_line, parse_request, read_content_length_frame,
    write_content_length_json, write_newline_json,
};
use serde_json::Value;
use std::io::{BufRead, BufReader};

pub(crate) fn run_stdio(server: &mut McpServer) -> Result<(), Box<dyn std::error::Error>> {
    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let mut stdout = std::io::stdout().lock();

    // Auto-detect framing once per process. This keeps responses consistent and avoids
    // interleaving different framing styles on the same transport.
    let mut mode: Option<TransportMode> = None;

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
                if mode.is_none() {
                    continue;
                }

                let detected = mode.unwrap();
                match detected {
                    TransportMode::NewlineJson => {
                        let raw = peek.trim();
                        if raw.is_empty() {
                            continue;
                        }
                        handle_body(server, &mut stdout, raw.as_bytes(), detected)?;
                        continue;
                    }
                    TransportMode::ContentLength => {
                        let Some(body) = read_content_length_frame(&mut reader, Some(peek))? else {
                            break;
                        };
                        handle_body(server, &mut stdout, &body, detected)?;
                        continue;
                    }
                }
            }
        };

        match effective_mode {
            TransportMode::NewlineJson => {
                let mut line = String::new();
                let read = reader.read_line(&mut line)?;
                if read == 0 {
                    break;
                }
                let raw = line.trim();
                if raw.is_empty() {
                    continue;
                }
                handle_body(server, &mut stdout, raw.as_bytes(), effective_mode)?;
            }
            TransportMode::ContentLength => {
                let mut first_header = String::new();
                let read = reader.read_line(&mut first_header)?;
                if read == 0 {
                    break;
                }
                if first_header.trim().is_empty() {
                    continue;
                }
                let Some(body) = read_content_length_frame(&mut reader, Some(first_header))? else {
                    break;
                };
                handle_body(server, &mut stdout, &body, effective_mode)?;
            }
        }
    }

    Ok(())
}

fn handle_body(
    server: &mut McpServer,
    stdout: &mut std::io::StdoutLock<'_>,
    body: &[u8],
    mode: TransportMode,
) -> Result<(), Box<dyn std::error::Error>> {
    let response: Option<Value> = match parse_request(body) {
        Ok(request) => server.handle(request),
        Err(err) => Some(err),
    };

    if let Some(resp) = response {
        match mode {
            TransportMode::NewlineJson => write_newline_json(stdout, &resp)?,
            TransportMode::ContentLength => write_content_length_json(stdout, &resp)?,
        }
    }

    Ok(())
}
