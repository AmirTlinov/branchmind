#![forbid(unsafe_code)]

use crate::WorkspaceId;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const DEFAULT_MAX_CHARS: usize = 8_192;
const HARD_MAX_CHARS: usize = 65_536;

#[derive(Clone, Debug)]
pub(crate) struct ParsedToolInput {
    pub(crate) workspace: String,
    pub(crate) max_chars: usize,
    pub(crate) command: ParsedCommand,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedCommand {
    pub(crate) verb: String,
    pub(crate) args: BTreeMap<String, String>,
    pub(crate) body: String,
}

impl ParsedCommand {
    pub(crate) fn require_arg(&self, name: &str) -> Result<String, Value> {
        self.args.get(name).cloned().ok_or_else(|| {
            parser_error(
                "INVALID_INPUT",
                &format!("{name} is required"),
                "Add the missing key=value pair to the bm command line.",
            )
        })
    }

    pub(crate) fn optional_arg(&self, name: &str) -> Option<&str> {
        self.args.get(name).map(|v| v.as_str())
    }

    pub(crate) fn optional_usize_arg(&self, name: &str, default: usize) -> Result<usize, Value> {
        let Some(raw) = self.optional_arg(name) else {
            return Ok(default);
        };
        raw.parse::<usize>().map_err(|_| {
            parser_error(
                "INVALID_INPUT",
                &format!("{name} must be an integer"),
                "Use a non-negative integer value.",
            )
        })
    }
}

pub(crate) fn parse_tool_markdown(
    args: Value,
    tool: &str,
    allowed_verbs: &[&str],
) -> Result<ParsedToolInput, Value> {
    let args_obj = args
        .as_object()
        .ok_or_else(|| parser_error("INVALID_INPUT", "arguments must be an object", "Use a JSON object for tool arguments."))?;

    let allowed_keys = BTreeSet::from([
        "workspace".to_string(),
        "markdown".to_string(),
        "max_chars".to_string(),
    ]);
    for key in args_obj.keys() {
        if !allowed_keys.contains(key) {
            return Err(parser_error(
                "UNKNOWN_ARG",
                &format!("Unknown argument: {key}"),
                "Use only workspace, markdown, and max_chars.",
            ));
        }
    }

    let workspace = args_obj
        .get("workspace")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            parser_error(
                "INVALID_INPUT",
                "workspace is required",
                "Provide workspace as a top-level string argument.",
            )
        })?;
    let workspace = WorkspaceId::try_new(workspace.to_string()).map_err(|_| {
        parser_error(
            "INVALID_INPUT",
            "workspace must be a valid WorkspaceId",
            "Use only letters, digits, '.', '-', '_' or '/'.",
        )
    })?;

    let max_chars = match args_obj.get("max_chars") {
        None | Some(Value::Null) => DEFAULT_MAX_CHARS,
        Some(Value::Number(v)) => {
            let Some(raw) = v.as_u64() else {
                return Err(parser_error(
                    "INVALID_INPUT",
                    "max_chars must be a positive integer",
                    "Set max_chars between 1 and 65536.",
                ));
            };
            let raw = usize::try_from(raw).unwrap_or(HARD_MAX_CHARS.saturating_add(1));
            if raw == 0 || raw > HARD_MAX_CHARS {
                return Err(parser_error(
                    "INVALID_INPUT",
                    "max_chars must be within [1, 65536]",
                    "Set max_chars between 1 and 65536.",
                ));
            }
            raw
        }
        Some(_) => {
            return Err(parser_error(
                "INVALID_INPUT",
                "max_chars must be a positive integer",
                "Set max_chars between 1 and 65536.",
            ))
        }
    };

    let markdown = args_obj
        .get("markdown")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            parser_error(
                "INVALID_INPUT",
                "markdown is required",
                "Provide one markdown string that contains exactly one ```bm fenced block.",
            )
        })?;

    if markdown.chars().count() > max_chars {
        return Err(parser_error(
            "BUDGET_EXCEEDED",
            "markdown exceeds max_chars",
            "Increase max_chars or shorten the markdown payload.",
        ));
    }

    let command = parse_command_block(markdown, tool, allowed_verbs)?;
    Ok(ParsedToolInput {
        workspace: workspace.as_str().to_string(),
        max_chars,
        command,
    })
}

fn parse_command_block(markdown: &str, tool: &str, allowed_verbs: &[&str]) -> Result<ParsedCommand, Value> {
    let normalized = markdown.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines: Vec<&str> = normalized.lines().collect();

    while lines.first().is_some_and(|line| line.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }

    if lines.is_empty() {
        return Err(parser_error(
            "INVALID_INPUT",
            "markdown is empty",
            "Use one fenced block: ```bm ... ```.",
        ));
    }

    if lines[0] != "```bm" {
        return Err(parser_error(
            "INVALID_INPUT",
            "markdown must start with ```bm",
            "Start the payload with a fenced bm block (```bm).",
        ));
    }

    let close_idx = lines
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(idx, line)| if *line == "```" { Some(idx) } else { None })
        .ok_or_else(|| {
            parser_error(
                "INVALID_INPUT",
                "missing closing ``` fence",
                "Close the bm fenced block with ``` on its own line.",
            )
        })?;

    if close_idx != lines.len() - 1 {
        return Err(parser_error(
            "INVALID_INPUT",
            "markdown must contain exactly one fenced bm block",
            "Keep only one bm block and remove all text outside the fence.",
        ));
    }

    let block_lines = &lines[1..close_idx];
    if block_lines.is_empty() {
        return Err(parser_error(
            "INVALID_INPUT",
            "bm block is empty",
            "Put command verb and args on the first line inside ```bm.",
        ));
    }

    let command_line = block_lines[0].trim();
    if command_line.is_empty() {
        return Err(parser_error(
            "INVALID_INPUT",
            "first bm line must contain verb and args",
            "Use format: `<verb> key=value` on the first line inside the block.",
        ));
    }

    let tokens = tokenize_command_line(command_line)?;
    if tokens.is_empty() {
        return Err(parser_error(
            "INVALID_INPUT",
            "first bm line must contain verb and args",
            "Use format: `<verb> key=value` on the first line inside the block.",
        ));
    }

    let verb = tokens[0].to_ascii_lowercase();
    if !is_valid_key(&verb) {
        return Err(parser_error(
            "INVALID_INPUT",
            "verb must be alphanumeric with '_' or '-'",
            "Use lowercase verb names like commit/create/into.",
        ));
    }
    if !allowed_verbs.iter().any(|allowed| *allowed == verb) {
        return Err(parser_error(
            "UNKNOWN_VERB",
            &format!("Unknown {tool} verb: {verb}"),
            &format!("Use tools/list and choose one of: {}.", allowed_verbs.join(", ")),
        ));
    }

    let mut args = BTreeMap::new();
    for token in tokens.iter().skip(1) {
        let Some((raw_key, raw_value)) = token.split_once('=') else {
            return Err(parser_error(
                "INVALID_INPUT",
                "command arguments must be key=value pairs",
                "Use `key=value` tokens after the verb.",
            ));
        };
        let key = raw_key.trim().to_ascii_lowercase();
        if !is_valid_key(&key) {
            return Err(parser_error(
                "INVALID_INPUT",
                &format!("invalid argument key: {raw_key}"),
                "Argument keys must match [a-zA-Z0-9_-].",
            ));
        }
        let value = raw_value.trim();
        if value.is_empty() {
            return Err(parser_error(
                "INVALID_INPUT",
                &format!("{key} must not be empty"),
                "Set a non-empty value on the right side of '='.",
            ));
        }
        if args.contains_key(&key) {
            return Err(parser_error(
                "INVALID_INPUT",
                &format!("duplicate argument: {key}"),
                "Each argument key may appear only once.",
            ));
        }
        args.insert(key, value.to_string());
    }

    let body = block_lines
        .iter()
        .skip(1)
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    Ok(ParsedCommand { verb, args, body })
}

fn tokenize_command_line(line: &str) -> Result<Vec<String>, Value> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escaped = false;

    for ch in line.chars() {
        if in_quotes {
            if escaped {
                match ch {
                    '\\' => current.push('\\'),
                    '"' => current.push('"'),
                    'n' => current.push('\n'),
                    't' => current.push('\t'),
                    other => {
                        return Err(parser_error(
                            "INVALID_INPUT",
                            &format!("unsupported escape sequence: \\{other}"),
                            "Use \\\\, \\\", \\n or \\t inside quoted values.",
                        ));
                    }
                }
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == '"' {
                in_quotes = false;
                continue;
            }
            current.push(ch);
            continue;
        }

        if ch.is_whitespace() {
            if !current.is_empty() {
                out.push(current.clone());
                current.clear();
            }
            continue;
        }

        if ch == '"' {
            in_quotes = true;
            continue;
        }

        current.push(ch);
    }

    if escaped {
        return Err(parser_error(
            "INVALID_INPUT",
            "unterminated escape sequence in command line",
            "Terminate escapes inside quoted values.",
        ));
    }
    if in_quotes {
        return Err(parser_error(
            "INVALID_INPUT",
            "unterminated quoted string in command line",
            "Close all quoted values with \".",
        ));
    }

    if !current.is_empty() {
        out.push(current);
    }

    Ok(out)
}

fn is_valid_key(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphanumeric() {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
}

fn parser_error(code: &str, message: &str, recovery: &str) -> Value {
    crate::ai_error_with(code, message, Some(recovery), Vec::new())
}
