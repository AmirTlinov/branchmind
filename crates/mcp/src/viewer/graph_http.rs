#![forbid(unsafe_code)]

use super::graph;
use super::{HttpRequest, ViewerConfig, ViewerStores};
use super::{decode_query_value, extract_query_param_raw, write_api_error, write_response};
use std::io;
use std::net::TcpStream;
use std::path::Path;

pub(crate) struct GraphHttpContext<'a> {
    pub(crate) stream: &'a mut TcpStream,
    pub(crate) stores: &'a mut ViewerStores,
    pub(crate) request_storage_dir: &'a Path,
    pub(crate) request_config: &'a ViewerConfig,
    pub(crate) workspace_override: Option<&'a str>,
    pub(crate) request: &'a HttpRequest,
    pub(crate) method: &'a str,
    pub(crate) path: &'a str,
    pub(crate) project_param_invalid: bool,
    pub(crate) project_unknown: bool,
}

fn ensure_get_or_head(method: &str, stream: &mut TcpStream) -> io::Result<()> {
    if method != "GET" && method != "HEAD" {
        return write_response(
            stream,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            b"Method not allowed.",
            false,
        );
    }
    Ok(())
}

fn ensure_project_guard(
    project_param_invalid: bool,
    project_unknown: bool,
    method: &str,
    stream: &mut TcpStream,
) -> io::Result<()> {
    if project_param_invalid {
        return write_api_error(
            stream,
            "400 Bad Request",
            "INVALID_PROJECT",
            "project: invalid project guard.",
            Some("Use a value like repo:0123abcd… from /api/projects."),
            method == "HEAD",
        );
    }
    if project_unknown {
        return write_api_error(
            stream,
            "404 Not Found",
            "UNKNOWN_PROJECT",
            "Unknown project.",
            Some("Pick one of the active projects returned by /api/projects."),
            method == "HEAD",
        );
    }
    Ok(())
}

fn open_store<'a>(
    stores: &'a mut ViewerStores,
    request_storage_dir: &Path,
    method: &str,
    stream: &mut TcpStream,
) -> io::Result<Option<&'a mut bm_storage::SqliteStore>> {
    match stores.store_for(request_storage_dir) {
        Ok(store) => Ok(Some(store)),
        Err(err) => {
            write_api_error(
                stream,
                "503 Service Unavailable",
                "PROJECT_UNAVAILABLE",
                "Unable to open project store in read-only mode.",
                Some(&format!("{err}")),
                method == "HEAD",
            )?;
            Ok(None)
        }
    }
}

fn parse_work_lens(
    request_path: &str,
    method: &str,
    stream: &mut TcpStream,
) -> io::Result<Option<&'static str>> {
    let lens_raw = extract_query_param_raw(request_path, "lens")
        .as_deref()
        .and_then(decode_query_value)
        .unwrap_or_else(|| "work".to_string());
    let lens = lens_raw.trim().to_ascii_lowercase();
    match lens.as_str() {
        "" | "work" => Ok(Some("work")),
        _ => {
            write_api_error(
                stream,
                "400 Bad Request",
                "INVALID_LENS",
                "lens: expected work.",
                Some("Remove lens=... or pass lens=work."),
                method == "HEAD",
            )?;
            Ok(None)
        }
    }
}

fn write_graph_result(
    stream: &mut TcpStream,
    method: &str,
    result: Result<serde_json::Value, super::snapshot::SnapshotError>,
) -> io::Result<()> {
    match result {
        Ok(payload) => {
            let body = payload.to_string();
            write_response(
                stream,
                "200 OK",
                "application/json; charset=utf-8",
                body.as_bytes(),
                method == "HEAD",
            )
        }
        Err(err) => {
            let body = err.to_json().to_string();
            write_response(
                stream,
                err.status_line(),
                "application/json; charset=utf-8",
                body.as_bytes(),
                method == "HEAD",
            )
        }
    }
}

pub(crate) fn handle_plan(ctx: GraphHttpContext<'_>) -> io::Result<()> {
    let GraphHttpContext {
        stream,
        stores,
        request_storage_dir,
        request_config,
        workspace_override,
        request,
        method,
        path,
        project_param_invalid,
        project_unknown,
    } = ctx;

    ensure_get_or_head(method, stream)?;
    ensure_project_guard(project_param_invalid, project_unknown, method, stream)?;
    let Some(lens) = parse_work_lens(&request.path, method, stream)? else {
        return Ok(());
    };

    let cursor = extract_query_param_raw(&request.path, "cursor")
        .as_deref()
        .and_then(decode_query_value);
    let limit = extract_query_param_raw(&request.path, "limit")
        .as_deref()
        .and_then(decode_query_value)
        .and_then(|value| value.parse::<usize>().ok());
    let plan_id = path.trim_start_matches("/api/graph/plan/").trim();

    let Some(store) = open_store(stores, request_storage_dir, method, stream)? else {
        return Ok(());
    };
    let result = graph::build_plan_subgraph(
        store,
        request_config,
        workspace_override,
        lens,
        plan_id,
        cursor.as_deref(),
        limit,
    );
    write_graph_result(stream, method, result)
}

pub(crate) fn handle_cluster(ctx: GraphHttpContext<'_>) -> io::Result<()> {
    let GraphHttpContext {
        stream,
        stores,
        request_storage_dir,
        request_config,
        workspace_override,
        request,
        method,
        path,
        project_param_invalid,
        project_unknown,
    } = ctx;

    ensure_get_or_head(method, stream)?;
    ensure_project_guard(project_param_invalid, project_unknown, method, stream)?;
    let Some(lens) = parse_work_lens(&request.path, method, stream)? else {
        return Ok(());
    };

    let cursor = extract_query_param_raw(&request.path, "cursor")
        .as_deref()
        .and_then(decode_query_value);
    let limit = extract_query_param_raw(&request.path, "limit")
        .as_deref()
        .and_then(decode_query_value)
        .and_then(|value| value.parse::<usize>().ok());
    let cluster_id = path.trim_start_matches("/api/graph/cluster/").trim();

    let Some(store) = open_store(stores, request_storage_dir, method, stream)? else {
        return Ok(());
    };
    let result = graph::build_cluster_subgraph(
        store,
        request_config,
        workspace_override,
        lens,
        cluster_id,
        cursor.as_deref(),
        limit,
    );
    write_graph_result(stream, method, result)
}

pub(crate) fn handle_local(ctx: GraphHttpContext<'_>) -> io::Result<()> {
    let GraphHttpContext {
        stream,
        stores,
        request_storage_dir,
        request_config,
        workspace_override,
        request,
        method,
        path,
        project_param_invalid,
        project_unknown,
    } = ctx;

    ensure_get_or_head(method, stream)?;
    ensure_project_guard(project_param_invalid, project_unknown, method, stream)?;
    let Some(lens) = parse_work_lens(&request.path, method, stream)? else {
        return Ok(());
    };

    let hops = extract_query_param_raw(&request.path, "hops")
        .as_deref()
        .and_then(decode_query_value)
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(2);
    let cursor = extract_query_param_raw(&request.path, "cursor")
        .as_deref()
        .and_then(decode_query_value);
    let limit = extract_query_param_raw(&request.path, "limit")
        .as_deref()
        .and_then(decode_query_value)
        .and_then(|value| value.parse::<usize>().ok());
    let node_id = path.trim_start_matches("/api/graph/local/").trim();

    let Some(store) = open_store(stores, request_storage_dir, method, stream)? else {
        return Ok(());
    };
    let result = graph::build_local_graph(
        store,
        request_config,
        workspace_override,
        graph::LocalGraphRequest {
            lens,
            node_id,
            hops,
            cursor: cursor.as_deref(),
            limit,
        },
    );
    write_graph_result(stream, method, result)
}
