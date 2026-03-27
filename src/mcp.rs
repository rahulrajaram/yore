use serde::Serialize;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use crate::assemble::*;
use crate::types::*;
use crate::util::*;

pub(crate) fn mcp_handle_dir(index_dir: &Path) -> PathBuf {
    index_dir.join("mcp_handles")
}

pub(crate) fn build_mcp_store_namespace(index_dir: &Path) -> String {
    const FNV_OFFSET_BASIS: u64 = 14_695_981_039_346_656_037;
    let canonical = canonicalize_existing_path(index_dir);
    let mut state = FNV_OFFSET_BASIS;
    stable_mcp_hash_update(&mut state, canonical.to_string_lossy().as_bytes());
    format!("{state:016x}")
}

pub(crate) fn fallback_mcp_handle_dir(index_dir: &Path) -> PathBuf {
    std::env::temp_dir()
        .join("yore")
        .join("mcp_handles")
        .join(build_mcp_store_namespace(index_dir))
}

pub(crate) fn candidate_mcp_handle_dirs(index_dir: &Path) -> Vec<PathBuf> {
    vec![
        mcp_handle_dir(index_dir),
        fallback_mcp_handle_dir(index_dir),
    ]
}

pub(crate) fn stable_mcp_hash_update(state: &mut u64, bytes: &[u8]) {
    const FNV_PRIME: u64 = 1_099_511_628_211;

    for byte in bytes {
        *state ^= u64::from(*byte);
        *state = state.wrapping_mul(FNV_PRIME);
    }
}

pub(crate) fn build_mcp_handle(query: &str, section: &SectionMatch) -> String {
    const FNV_OFFSET_BASIS: u64 = 14_695_981_039_346_656_037;
    let mut state = FNV_OFFSET_BASIS;

    stable_mcp_hash_update(&mut state, query.as_bytes());
    stable_mcp_hash_update(&mut state, &[0xff]);
    stable_mcp_hash_update(&mut state, section.doc_path.as_bytes());
    stable_mcp_hash_update(&mut state, &[0xff]);
    stable_mcp_hash_update(&mut state, section.heading.as_bytes());
    stable_mcp_hash_update(&mut state, &[0xff]);
    stable_mcp_hash_update(&mut state, &section.line_start.to_le_bytes());
    stable_mcp_hash_update(&mut state, &[0xff]);
    stable_mcp_hash_update(&mut state, &section.line_end.to_le_bytes());
    stable_mcp_hash_update(&mut state, &[0xff]);
    stable_mcp_hash_update(&mut state, section.content.as_bytes());

    format!("ctx_{state:016x}")
}

pub(crate) fn build_mcp_source_ref(section: &SectionMatch) -> McpSourceRef {
    McpSourceRef {
        path: section.doc_path.clone(),
        heading: section.heading.clone(),
        line_start: section.line_start,
        line_end: section.line_end,
    }
}

pub(crate) fn store_mcp_artifact(
    index_dir: &Path,
    artifact: &McpArtifact,
) -> Result<(), Box<dyn std::error::Error>> {
    let payload = serde_json::to_vec_pretty(artifact)?;
    let mut last_error: Option<io::Error> = None;

    for handle_dir in candidate_mcp_handle_dirs(index_dir) {
        match fs::create_dir_all(&handle_dir) {
            Ok(()) => {}
            Err(err) => {
                last_error = Some(err);
                continue;
            }
        }

        let handle_path = handle_dir.join(format!("{}.json", artifact.handle));
        match fs::write(handle_path, &payload) {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_error = Some(err);
            }
        }
    }

    Err(last_error
        .unwrap_or_else(|| io::Error::other("unable to store MCP artifact"))
        .into())
}

pub(crate) fn load_mcp_artifact(
    index_dir: &Path,
    handle: &str,
) -> Result<McpArtifact, Box<dyn std::error::Error>> {
    let mut last_error: Option<io::Error> = None;

    for handle_dir in candidate_mcp_handle_dirs(index_dir) {
        let handle_path = handle_dir.join(format!("{handle}.json"));
        match fs::read_to_string(&handle_path) {
            Ok(content) => return Ok(serde_json::from_str(&content)?),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                last_error = Some(err);
            }
            Err(err) => return Err(err.into()),
        }
    }

    Err(last_error
        .unwrap_or_else(|| io::Error::new(io::ErrorKind::NotFound, "unknown handle"))
        .into())
}

pub(crate) fn build_mcp_search_response(
    query: &str,
    from_files: &[String],
    index_dir: &Path,
    options: McpSearchOptions,
) -> Result<McpSearchResponse, Box<dyn std::error::Error>> {
    let forward_index = load_forward_index(index_dir)?;
    let selection_mode = if from_files.is_empty() {
        "query".to_string()
    } else {
        "from_files".to_string()
    };
    let requested_query = if query.trim().is_empty() {
        "selected files".to_string()
    } else {
        query.to_string()
    };

    let selection_limit = options.max_results.max(1).saturating_mul(4).max(8);
    let selection = match collect_context_selection(
        query,
        from_files,
        &forward_index,
        selection_limit,
    ) {
        Ok(selection) => selection,
        Err(issue) => {
            let (error, message, missing_files) = match issue {
                ContextSelectionIssue::NoSearchableTerms => (
                    Some("no_query_terms".to_string()),
                    Some("No searchable terms in query. Try different keywords.".to_string()),
                    Vec::new(),
                ),
                ContextSelectionIssue::MissingFiles(missing) => (
                    Some("missing_files".to_string()),
                    Some(
                        "Some files were not found in the index; search-context requires explicit indexed files."
                            .to_string(),
                    ),
                    missing,
                ),
                ContextSelectionIssue::NoIndexedFilesMatched => (
                    Some("no_indexed_files".to_string()),
                    Some("No indexed files matched the provided inputs.".to_string()),
                    Vec::new(),
                ),
                ContextSelectionIssue::NoRelevantSections(label) => (
                    Some("no_relevant_sections".to_string()),
                    Some(format!("No relevant sections found for query: \"{label}\"")),
                    Vec::new(),
                ),
            };

            return Ok(McpSearchResponse {
                schema_version: MCP_SCHEMA_VERSION,
                tool: "search_context".to_string(),
                query: requested_query,
                selection_mode,
                budget: McpSearchBudget {
                    max_results: options.max_results,
                    max_tokens: options.max_tokens,
                    max_bytes: options.max_bytes,
                    ..McpSearchBudget::default()
                },
                pressure: McpPressure::default(),
                results: Vec::new(),
                error,
                message,
                missing_files,
            });
        }
    };

    let (unique_sections, deduped_hits) = dedupe_section_matches(selection.sections.clone());
    let max_results = options.max_results.max(1);
    let per_result_tokens = (options.max_tokens / max_results).max(40);
    let per_result_bytes = (options.max_bytes / max_results).max(160);
    let preview_sections = apply_extractive_refiner(
        unique_sections.clone(),
        &selection.query_for_refiner,
        per_result_tokens,
    );

    let mut pressure = McpPressure::default();
    let mut budget = McpSearchBudget {
        max_results: options.max_results,
        max_tokens: options.max_tokens,
        max_bytes: options.max_bytes,
        candidate_hits: selection.sections.len(),
        deduped_hits,
        ..McpSearchBudget::default()
    };
    let mut results = Vec::new();
    let mut used_tokens = 0usize;
    let mut used_bytes = 0usize;

    for (rank, (raw_section, preview_section)) in unique_sections
        .iter()
        .zip(preview_sections.iter())
        .enumerate()
    {
        if results.len() >= max_results {
            pressure.truncated = true;
            pressure.reasons.push("result_cap".to_string());
            break;
        }

        let (preview, truncated, truncation_reasons) = truncate_text_to_budget(
            &preview_section.section.content,
            per_result_tokens,
            per_result_bytes,
        );
        let preview_tokens = estimate_tokens(&preview);
        let preview_bytes = preview.len();
        let mut result_truncated = preview_section.truncated || truncated;
        let mut result_reasons = preview_section.truncation_reasons.clone();
        result_reasons.extend(truncation_reasons.clone());

        if used_tokens + preview_tokens > options.max_tokens {
            pressure.truncated = true;
            pressure.reasons.push("token_cap".to_string());
            break;
        }
        if used_bytes + preview_bytes > options.max_bytes {
            pressure.truncated = true;
            pressure.reasons.push("byte_cap".to_string());
            break;
        }

        result_reasons.sort();
        result_reasons.dedup();
        result_truncated = result_truncated || !result_reasons.is_empty();

        if result_truncated {
            pressure.truncated = true;
            pressure.reasons.extend(result_reasons.clone());
        }

        let handle = build_mcp_handle(&selection.query_label, raw_section);
        let artifact = McpArtifact {
            schema_version: MCP_SCHEMA_VERSION,
            handle: handle.clone(),
            query: selection.query_label.clone(),
            source: build_mcp_source_ref(raw_section),
            scores: McpScoreBreakdown {
                bm25: raw_section.bm25_score,
                canonicality: raw_section.canonicality,
                combined: combined_section_score(raw_section),
            },
            preview: preview.clone(),
            content: raw_section.content.clone(),
            created_at: chrono_now(),
        };
        if let Err(err) = store_mcp_artifact(index_dir, &artifact) {
            return Ok(McpSearchResponse {
                schema_version: MCP_SCHEMA_VERSION,
                tool: "search_context".to_string(),
                query: selection.query_label.clone(),
                selection_mode: selection_mode.clone(),
                budget: McpSearchBudget {
                    returned_results: results.len(),
                    estimated_tokens: used_tokens,
                    bytes: used_bytes,
                    ..budget
                },
                pressure,
                results,
                error: Some("artifact_store_unavailable".to_string()),
                message: Some(format!(
                    "Unable to persist MCP handles for follow-up fetches: {err}"
                )),
                missing_files: Vec::new(),
            });
        }

        results.push(McpSearchResult {
            handle,
            rank: rank + 1,
            source: artifact.source.clone(),
            scores: artifact.scores.clone(),
            preview,
            preview_tokens,
            preview_bytes,
            truncated: result_truncated,
            truncation_reasons: result_reasons,
        });

        used_tokens += preview_tokens;
        used_bytes += preview_bytes;
    }

    budget.returned_results = results.len();
    budget.omitted_hits = unique_sections.len().saturating_sub(results.len());
    budget.estimated_tokens = used_tokens;
    budget.bytes = used_bytes;

    if budget.omitted_hits > 0 && !pressure.reasons.iter().any(|reason| reason == "result_cap") {
        pressure.truncated = true;
        pressure.reasons.push("result_cap".to_string());
    }
    pressure.reasons.sort();
    pressure.reasons.dedup();

    Ok(McpSearchResponse {
        schema_version: MCP_SCHEMA_VERSION,
        tool: "search_context".to_string(),
        query: selection.query_label,
        selection_mode,
        budget,
        pressure,
        results,
        error: None,
        message: None,
        missing_files: Vec::new(),
    })
}

pub(crate) fn cmd_mcp_search_context(
    query: &str,
    from_files: &[String],
    index_dir: &Path,
    options: McpSearchOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let response = build_mcp_search_response(query, from_files, index_dir, options)?;
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}

pub(crate) fn build_mcp_fetch_response(
    handle: &str,
    index_dir: &Path,
    options: McpFetchOptions,
) -> Result<McpFetchResponse, Box<dyn std::error::Error>> {
    let Ok(artifact) = load_mcp_artifact(index_dir, handle) else {
        return Ok(McpFetchResponse {
            schema_version: MCP_SCHEMA_VERSION,
            tool: "fetch_context".to_string(),
            handle: handle.to_string(),
            budget: McpFetchBudget {
                max_tokens: options.max_tokens,
                max_bytes: options.max_bytes,
                ..McpFetchBudget::default()
            },
            pressure: McpPressure::default(),
            query: None,
            result: None,
            error: Some("unknown_handle".to_string()),
            message: Some(format!(
                "No stored MCP artifact found for handle '{handle}'. Run `yore mcp search-context` first."
            )),
        });
    };

    let (content, truncated, truncation_reasons) =
        truncate_text_to_budget(&artifact.content, options.max_tokens, options.max_bytes);
    let content_tokens = estimate_tokens(&content);
    let content_bytes = content.len();

    Ok(McpFetchResponse {
        schema_version: MCP_SCHEMA_VERSION,
        tool: "fetch_context".to_string(),
        handle: handle.to_string(),
        budget: McpFetchBudget {
            max_tokens: options.max_tokens,
            max_bytes: options.max_bytes,
            estimated_tokens: content_tokens,
            bytes: content_bytes,
        },
        pressure: McpPressure {
            truncated,
            reasons: truncation_reasons,
        },
        query: Some(artifact.query),
        result: Some(McpFetchResult {
            source: artifact.source,
            scores: artifact.scores,
            preview: artifact.preview,
            content,
            content_tokens,
            content_bytes,
        }),
        error: None,
        message: None,
    })
}

pub(crate) fn cmd_mcp_fetch_context(
    handle: &str,
    index_dir: &Path,
    options: McpFetchOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let response = build_mcp_fetch_response(handle, index_dir, options)?;
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}

pub(crate) fn read_mcp_stdio_message<R: BufRead>(
    reader: &mut R,
) -> Result<Option<serde_json::Value>, io::Error> {
    let mut content_length: Option<usize> = None;
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            if content_length.is_none() {
                return Ok(None);
            }
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "unexpected EOF while reading MCP message headers",
            ));
        }

        if line == "\r\n" || line == "\n" {
            break;
        }

        let header = line.trim_end_matches(['\r', '\n']);
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = Some(value.trim().parse().map_err(|err| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("invalid Content-Length header: {err}"),
                    )
                })?);
            }
        }
    }

    let content_length = content_length.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "missing Content-Length header in MCP message",
        )
    })?;
    let mut payload = vec![0; content_length];
    reader.read_exact(&mut payload)?;
    serde_json::from_slice(&payload)
        .map(Some)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

pub(crate) fn write_mcp_stdio_message<W: Write, T: Serialize>(
    writer: &mut W,
    payload: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    let body = serde_json::to_vec(payload)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

pub(crate) fn resolve_mcp_tool_index(
    default_index: &Path,
    requested_index: Option<PathBuf>,
) -> PathBuf {
    requested_index.unwrap_or_else(|| default_index.to_path_buf())
}

pub(crate) fn mcp_tool_definitions() -> serde_json::Value {
    serde_json::json!([
        {
            "name": "search_context",
            "description": "Return bounded previews, source references, pressure metadata, and opaque handles for explicit follow-up fetches.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Natural language query or question. Optional when from_files is provided."
                    },
                    "from_files": {
                        "type": "array",
                        "description": "Explicit indexed files to preview instead of a query. Supports @list.txt expansion.",
                        "items": {
                            "type": "string"
                        },
                        "minItems": 1
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum preview results to return.",
                        "minimum": 1,
                        "default": 5
                    },
                    "max_tokens": {
                        "type": "integer",
                        "description": "Approximate maximum total tokens across previews.",
                        "minimum": 1,
                        "default": 1200
                    },
                    "max_bytes": {
                        "type": "integer",
                        "description": "Maximum total bytes across previews.",
                        "minimum": 1,
                        "default": 12000
                    },
                    "index": {
                        "type": "string",
                        "description": "Optional override for the index directory. Defaults to the server's configured index."
                    }
                },
                "oneOf": [
                    {
                        "required": ["query"]
                    },
                    {
                        "required": ["from_files"]
                    }
                ]
            }
        },
        {
            "name": "fetch_context",
            "description": "Expand a previously returned opaque handle with its own token and byte caps.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "handle": {
                        "type": "string",
                        "description": "Opaque ctx_... handle returned by search_context."
                    },
                    "max_tokens": {
                        "type": "integer",
                        "description": "Approximate maximum tokens for fetched content.",
                        "minimum": 1,
                        "default": 4000
                    },
                    "max_bytes": {
                        "type": "integer",
                        "description": "Maximum bytes for fetched content.",
                        "minimum": 1,
                        "default": 20000
                    },
                    "index": {
                        "type": "string",
                        "description": "Optional override for the index directory. Defaults to the server's configured index."
                    }
                },
                "required": ["handle"]
            }
        }
    ])
}

pub(crate) fn build_mcp_tool_result<T: Serialize>(
    payload: &T,
    is_error: bool,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    Ok(serde_json::json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string(payload)?,
            }
        ],
        "structuredContent": serde_json::to_value(payload)?,
        "isError": is_error,
    }))
}

pub(crate) fn json_rpc_success(
    id: serde_json::Value,
    result: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

pub(crate) fn json_rpc_error(
    id: Option<serde_json::Value>,
    code: i64,
    message: &str,
) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(serde_json::Value::Null),
        "error": {
            "code": code,
            "message": message,
        }
    })
}

pub(crate) fn cmd_mcp_serve(index_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    loop {
        let Some(message) = read_mcp_stdio_message(&mut reader)? else {
            break;
        };
        let request: JsonRpcRequest = match serde_json::from_value(message) {
            Ok(request) => request,
            Err(err) => {
                let response = json_rpc_error(None, -32600, &format!("Invalid request: {err}"));
                write_mcp_stdio_message(&mut writer, &response)?;
                continue;
            }
        };

        if request.jsonrpc.as_deref() != Some("2.0") {
            if let Some(id) = request.id {
                let response = json_rpc_error(Some(id), -32600, "Only JSON-RPC 2.0 is supported.");
                write_mcp_stdio_message(&mut writer, &response)?;
            }
            continue;
        }

        let response = match request.method.as_str() {
            "initialize" => {
                let params: McpInitializeParams = serde_json::from_value(request.params)
                    .unwrap_or_else(|_| McpInitializeParams::default());
                let protocol_version = params
                    .protocol_version
                    .unwrap_or_else(|| DEFAULT_MCP_PROTOCOL_VERSION.to_string());
                request.id.map(|id| {
                    json_rpc_success(
                        id,
                        serde_json::json!({
                            "protocolVersion": protocol_version,
                            "capabilities": {
                                "tools": {
                                    "listChanged": false
                                }
                            },
                            "serverInfo": {
                                "name": "yore",
                                "version": env!("CARGO_PKG_VERSION")
                            },
                            "instructions": "Use search_context for bounded previews and fetch_context only for explicit follow-up expansion.",
                        }),
                    )
                })
            }
            "notifications/initialized" | "notifications/cancelled" => None,
            "ping" => request
                .id
                .map(|id| json_rpc_success(id, serde_json::json!({}))),
            "tools/list" => request.id.map(|id| {
                json_rpc_success(
                    id,
                    serde_json::json!({
                        "tools": mcp_tool_definitions(),
                    }),
                )
            }),
            "tools/call" => {
                let id = request.id.clone();
                match serde_json::from_value::<McpToolCallParams>(request.params) {
                    Ok(McpToolCallParams { name, arguments }) => match name.as_str() {
                        "search_context" => {
                            match serde_json::from_value::<McpSearchToolArgs>(arguments) {
                                Ok(args) => {
                                    let tool_index = resolve_mcp_tool_index(index_dir, args.index);
                                    let response = build_mcp_search_response(
                                        args.query.trim(),
                                        &args.from_files,
                                        &tool_index,
                                        McpSearchOptions {
                                            max_results: args.max_results,
                                            max_tokens: args.max_tokens,
                                            max_bytes: args.max_bytes,
                                        },
                                    )?;
                                    let result =
                                        build_mcp_tool_result(&response, response.error.is_some())?;
                                    id.map(|id| json_rpc_success(id, result))
                                }
                                Err(err) => id.map(|id| {
                                    json_rpc_error(
                                        Some(id),
                                        -32602,
                                        &format!("Invalid search_context arguments: {err}"),
                                    )
                                }),
                            }
                        }
                        "fetch_context" => {
                            match serde_json::from_value::<McpFetchToolArgs>(arguments) {
                                Ok(args) => {
                                    let tool_index = resolve_mcp_tool_index(index_dir, args.index);
                                    let response = build_mcp_fetch_response(
                                        args.handle.trim(),
                                        &tool_index,
                                        McpFetchOptions {
                                            max_tokens: args.max_tokens,
                                            max_bytes: args.max_bytes,
                                        },
                                    )?;
                                    let result =
                                        build_mcp_tool_result(&response, response.error.is_some())?;
                                    id.map(|id| json_rpc_success(id, result))
                                }
                                Err(err) => id.map(|id| {
                                    json_rpc_error(
                                        Some(id),
                                        -32602,
                                        &format!("Invalid fetch_context arguments: {err}"),
                                    )
                                }),
                            }
                        }
                        _ => id.map(|id| {
                            json_rpc_error(Some(id), -32602, &format!("Unknown tool '{name}'."))
                        }),
                    },
                    Err(err) => id.map(|id| {
                        json_rpc_error(
                            Some(id),
                            -32602,
                            &format!("Invalid tools/call params: {err}"),
                        )
                    }),
                }
            }
            _ => request.id.map(|id| {
                json_rpc_error(
                    Some(id),
                    -32601,
                    &format!("Method '{}' is not supported.", request.method),
                )
            }),
        };

        if let Some(response) = response {
            write_mcp_stdio_message(&mut writer, &response)?;
        }
    }

    Ok(())
}
