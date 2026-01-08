#![forbid(unsafe_code)]

use crate::json_rpc_error;
use serde_json::Value;
use std::io::{BufRead, Write};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransportMode {
    NewlineJson,
    ContentLength,
}

pub(crate) fn detect_mode_from_first_line(line: &str) -> Option<TransportMode> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Some(TransportMode::NewlineJson);
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("content-length:") || lower.starts_with("content-type:") {
        return Some(TransportMode::ContentLength);
    }
    None
}

pub(crate) fn parse_content_length_header(line: &str) -> Option<usize> {
    let trimmed = line.trim();
    let (key, value) = trimmed.split_once(':')?;
    if !key.trim().eq_ignore_ascii_case("content-length") {
        return None;
    }
    value.trim().parse::<usize>().ok()
}

pub(crate) fn read_content_length_frame<R: BufRead>(
    reader: &mut R,
    mut first_header: Option<String>,
) -> std::io::Result<Option<Vec<u8>>> {
    const MAX_CONTENT_LENGTH_BYTES: usize = 16 * 1024 * 1024;

    let mut header = String::new();
    if let Some(seed) = first_header.take() {
        header = seed;
    } else {
        let read = reader.read_line(&mut header)?;
        if read == 0 {
            return Ok(None);
        }
    }

    let mut content_length: Option<usize> = parse_content_length_header(&header);

    loop {
        let trimmed = header.trim_end();
        if trimmed.is_empty() {
            break;
        }

        header.clear();
        let read = reader.read_line(&mut header)?;
        if read == 0 {
            return Ok(None);
        }

        if content_length.is_none() {
            content_length = parse_content_length_header(&header);
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

pub(crate) fn write_newline_json<W: Write>(
    writer: &mut W,
    resp: &Value,
) -> Result<(), Box<dyn std::error::Error>> {
    writeln!(writer, "{}", serde_json::to_string(resp)?)?;
    writer.flush()?;
    Ok(())
}

pub(crate) fn write_content_length_json<W: Write>(
    writer: &mut W,
    resp: &Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let body = serde_json::to_vec(resp)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

pub(crate) fn request_expects_response(body: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return true;
    };
    let Some(obj) = value.as_object() else {
        return true;
    };
    match obj.get("id") {
        Some(Value::Null) | None => false,
        _ => true,
    }
}

pub(crate) fn parse_request(body: &[u8]) -> Result<crate::JsonRpcRequest, Value> {
    let data: Value = serde_json::from_slice(body).map_err(|e| {
        json_rpc_error(None, -32700, &format!("Parse error: {e}"))
    })?;

    let (id, has_method) = match data.as_object() {
        Some(obj) => (obj.get("id").cloned(), obj.contains_key("method")),
        None => {
            return Err(json_rpc_error(None, -32600, "Invalid Request"));
        }
    };
    if !has_method {
        return Err(json_rpc_error(id, -32600, "Invalid Request"));
    }

    serde_json::from_value::<crate::JsonRpcRequest>(data).map_err(|e| {
        json_rpc_error(id, -32600, &format!("Invalid Request: {e}"))
    })
}
