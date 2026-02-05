#![forbid(unsafe_code)]

use crate::now_ms_i64;
use bm_core::ids::WorkspaceId;
use bm_storage::SqliteStore;
use serde_json::json;
use std::io::Write;
use std::net::TcpStream;
use std::path::PathBuf;

pub(crate) struct EventsStreamStartOptions {
    pub(crate) poll_ms: i64,
    pub(crate) keepalive_ms: i64,
    pub(crate) max_events: usize,
    pub(crate) max_stream_ms: i64,
    pub(crate) project_guard: Option<String>,
}

pub(crate) struct EventsStreamClient {
    pub(crate) stream: TcpStream,
    pub(crate) storage_dir: PathBuf,
    pub(crate) workspace: WorkspaceId,
    pub(crate) last_event_id: String,
    pub(crate) started_at_ms: i64,
    pub(crate) last_poll_ms: i64,
    pub(crate) last_keepalive_ms: i64,
    pub(crate) poll_ms: i64,
    pub(crate) keepalive_ms: i64,
    pub(crate) max_events: usize,
    pub(crate) max_stream_ms: i64,
    pub(crate) events_sent: usize,
}

impl EventsStreamClient {
    pub(crate) fn start(
        mut stream: TcpStream,
        storage_dir: PathBuf,
        workspace: WorkspaceId,
        last_event_id: String,
        opts: EventsStreamStartOptions,
    ) -> std::io::Result<Self> {
        let _ = stream.set_write_timeout(Some(std::time::Duration::from_millis(250)));
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(250)));

        write_events_stream_headers(&mut stream)?;

        // Tell EventSource how long to wait before reconnecting after the server closes the stream.
        // (We intentionally enforce budgets and close connections periodically.)
        stream.write_all(b"retry: 3000\n\n")?;

        let now_ms = now_ms_i64();
        let event_id = last_event_id.clone();
        let ready_payload = json!({
            "generated_at_ms": now_ms,
            "workspace": workspace.as_str(),
            "project_guard": opts.project_guard.as_deref(),
            "event_id": event_id,
        })
        .to_string();
        write_sse_event(&mut stream, Some(&last_event_id), "ready", &ready_payload)?;

        Ok(Self {
            stream,
            storage_dir,
            workspace,
            last_event_id,
            started_at_ms: now_ms,
            last_poll_ms: now_ms,
            last_keepalive_ms: now_ms,
            poll_ms: opts.poll_ms.max(25),
            keepalive_ms: opts.keepalive_ms.max(1_000),
            max_events: opts.max_events.max(1),
            max_stream_ms: opts.max_stream_ms.max(5_000),
            events_sent: 0,
        })
    }

    pub(crate) fn tick(&mut self, store: &mut SqliteStore, now_ms: i64) -> std::io::Result<bool> {
        if now_ms.saturating_sub(self.started_at_ms) >= self.max_stream_ms {
            let payload = json!({ "reason": "max_stream_ms" }).to_string();
            let _ = write_sse_event(&mut self.stream, Some(&self.last_event_id), "eof", &payload);
            return Ok(false);
        }
        if self.events_sent >= self.max_events {
            let payload = json!({ "reason": "max_events" }).to_string();
            let _ = write_sse_event(&mut self.stream, Some(&self.last_event_id), "eof", &payload);
            return Ok(false);
        }

        if now_ms.saturating_sub(self.last_keepalive_ms) >= self.keepalive_ms {
            let line = format!(": keepalive {now_ms}\n\n");
            if self.stream.write_all(line.as_bytes()).is_err() {
                return Ok(false);
            }
            self.last_keepalive_ms = now_ms;
        }

        if now_ms.saturating_sub(self.last_poll_ms) < self.poll_ms {
            return Ok(true);
        }
        self.last_poll_ms = now_ms;

        let remaining = self.max_events.saturating_sub(self.events_sent);
        if remaining == 0 {
            return Ok(true);
        }
        let limit = remaining.min(50);

        let events = match store.list_events(&self.workspace, Some(&self.last_event_id), limit) {
            Ok(events) => events,
            Err(_) => return Ok(false),
        };

        for event in events {
            let event_id = event.event_id();
            let payload = json!({
                "event_id": event_id,
                "seq": event.seq,
                "ts_ms": event.ts_ms,
                "task_id": event.task_id,
                "path": event.path,
                "type": event.event_type,
            })
            .to_string();

            if write_sse_event(&mut self.stream, Some(&event_id), "bm_event", &payload).is_err() {
                return Ok(false);
            }
            self.last_event_id = event_id;
            self.events_sent += 1;

            if self.events_sent >= self.max_events {
                break;
            }
        }

        Ok(true)
    }
}

pub(crate) fn write_events_stream_headers(stream: &mut TcpStream) -> std::io::Result<()> {
    // NOTE: we do not set Content-Length, as the stream is long-lived.
    // We also keep the CSP relaxed to the same baseline as other endpoints.
    let headers = concat!(
        "HTTP/1.1 200 OK\r\n",
        "Content-Type: text/event-stream; charset=utf-8\r\n",
        "Cache-Control: no-store\r\n",
        "X-Content-Type-Options: nosniff\r\n",
        "Content-Security-Policy: default-src 'self'; style-src 'self'; script-src 'self'; img-src 'self' data:;\r\n",
        "Connection: keep-alive\r\n",
        "\r\n"
    );
    stream.write_all(headers.as_bytes())
}

fn write_sse_event(
    stream: &mut TcpStream,
    id: Option<&str>,
    event: &str,
    data: &str,
) -> std::io::Result<()> {
    // SSE lines use LF (`\n`) rather than CRLF.
    if let Some(id) = id {
        writeln!(stream, "id: {id}")?;
    }
    writeln!(stream, "event: {event}")?;
    for line in data.lines() {
        writeln!(stream, "data: {line}")?;
    }
    writeln!(stream)?;
    Ok(())
}
