use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
#[cfg(unix)]
use std::{fs::Permissions, os::unix::fs::PermissionsExt};

fn temp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("yore-mcp-server-test-{}-{}", label, nanos));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_cmd(mut cmd: Command) -> (bool, String) {
    let output = cmd.output().expect("command failed to start");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    (output.status.success(), stdout)
}

fn long_auth_doc() -> String {
    let mut doc = String::from(
        "# Authentication Overview\n\n\
Authentication flow validates credentials against the identity store and issues a session token.\n\
Every successful login records an audit event and includes the actor, scope, and timestamp.\n\
If validation fails, the service records a denial event and returns an access failure.\n\n",
    );

    for idx in 0..12 {
        doc.push_str(&format!(
            "Authentication step {} keeps the audit trail consistent and explains why session revocation happens after suspicious activity.\n",
            idx + 1
        ));
    }

    doc
}

fn write_docs(root: &Path) {
    let docs = root.join("docs");
    fs::create_dir_all(&docs).unwrap();

    let auth = long_auth_doc();
    fs::write(docs.join("aa-auth.md"), &auth).unwrap();
    fs::write(docs.join("zz-auth-copy.md"), &auth).unwrap();
    fs::write(
        docs.join("ops.md"),
        "# Operations\n\nDeployment runbook for maintenance windows.\n",
    )
    .unwrap();
}

fn build_index(root: &Path, index_dir: &Path) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yore"));
    cmd.current_dir(root)
        .args(["build", "docs", "--output"])
        .arg(index_dir);
    let (ok, stdout) = run_cmd(cmd);
    assert!(ok, "build failed: {}", stdout);
}

fn write_mcp_message(stdin: &mut ChildStdin, payload: &Value) {
    let body = serde_json::to_vec(payload).unwrap();
    write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
    stdin.write_all(&body).unwrap();
    stdin.flush().unwrap();
}

fn read_mcp_message(stdout: &mut BufReader<ChildStdout>) -> Value {
    let mut content_length = None;
    let mut line = String::new();

    loop {
        line.clear();
        let bytes = stdout.read_line(&mut line).unwrap();
        assert!(bytes > 0, "unexpected EOF while reading MCP headers");
        if line == "\r\n" || line == "\n" {
            break;
        }
        let header = line.trim_end_matches(['\r', '\n']);
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = Some(value.trim().parse::<usize>().unwrap());
            }
        }
    }

    let mut payload = vec![0; content_length.expect("missing Content-Length")];
    stdout.read_exact(&mut payload).unwrap();
    serde_json::from_slice(&payload).unwrap()
}

struct McpServerHarness {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpServerHarness {
    fn start(root: &Path, index_dir: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_yore"))
            .current_dir(root)
            .args(["mcp", "serve", "--index"])
            .arg(index_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("failed to start MCP server");

        let stdin = child.stdin.take().expect("missing stdin");
        let stdout = BufReader::new(child.stdout.take().expect("missing stdout"));

        Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        write_mcp_message(
            &mut self.stdin,
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params,
            }),
        );
        read_mcp_message(&mut self.stdout)
    }

    fn notify(&mut self, method: &str, params: Value) {
        write_mcp_message(
            &mut self.stdin,
            &json!({
                "jsonrpc": "2.0",
                "method": method,
                "params": params,
            }),
        );
    }

    fn initialize(&mut self) -> Value {
        let response = self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {
                    "name": "yore-test",
                    "version": "1.0"
                }
            }),
        );
        self.notify("notifications/initialized", json!({}));
        response
    }
}

impl Drop for McpServerHarness {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn test_mcp_serve_lists_tools_and_supports_search_then_fetch() {
    let root = temp_dir("search-fetch");
    write_docs(&root);
    let index_dir = root.join(".yore-test");
    build_index(&root, &index_dir);

    let mut server = McpServerHarness::start(&root, &index_dir);

    let init = server.initialize();
    assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(init["result"]["serverInfo"]["name"], "yore");
    assert_eq!(
        init["result"]["capabilities"]["tools"]["listChanged"],
        false
    );

    let tools = server.request("tools/list", json!({}));
    let tool_names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(tool_names.contains(&"search_context"));
    assert!(tool_names.contains(&"fetch_context"));

    let search = server.request(
        "tools/call",
        json!({
            "name": "search_context",
            "arguments": {
                "query": "authentication",
                "max_results": 2,
                "max_tokens": 160,
                "max_bytes": 700
            }
        }),
    );
    assert_eq!(search["result"]["isError"], false);
    let search_payload = &search["result"]["structuredContent"];
    assert_eq!(search_payload["tool"], "search_context");
    assert_eq!(search_payload["results"].as_array().unwrap().len(), 1);
    assert!(search["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("\"tool\":\"search_context\""));
    let handle = search_payload["results"][0]["handle"]
        .as_str()
        .unwrap()
        .to_string();

    let fetch = server.request(
        "tools/call",
        json!({
            "name": "fetch_context",
            "arguments": {
                "handle": handle,
                "max_tokens": 40,
                "max_bytes": 220
            }
        }),
    );
    assert_eq!(fetch["result"]["isError"], false);
    let fetch_payload = &fetch["result"]["structuredContent"];
    assert_eq!(fetch_payload["tool"], "fetch_context");
    assert!(fetch_payload["pressure"]["truncated"].as_bool().unwrap());
    assert!(fetch_payload["result"]["content"]
        .as_str()
        .unwrap()
        .contains("[truncated]"));
}

#[cfg(unix)]
#[test]
fn test_mcp_serve_uses_read_only_handle_fallback() {
    let root = temp_dir("readonly");
    write_docs(&root);
    let index_dir = root.join(".yore-test");
    build_index(&root, &index_dir);
    fs::set_permissions(&index_dir, Permissions::from_mode(0o555)).unwrap();

    let mut server = McpServerHarness::start(&root, &index_dir);
    server.initialize();

    let search = server.request(
        "tools/call",
        json!({
            "name": "search_context",
            "arguments": {
                "query": "authentication",
                "max_results": 1,
                "max_tokens": 120,
                "max_bytes": 500
            }
        }),
    );
    assert_eq!(search["result"]["isError"], false);
    let search_payload = &search["result"]["structuredContent"];
    let handle = search_payload["results"][0]["handle"]
        .as_str()
        .unwrap()
        .to_string();

    let fetch = server.request(
        "tools/call",
        json!({
            "name": "fetch_context",
            "arguments": {
                "handle": handle
            }
        }),
    );
    assert_eq!(fetch["result"]["isError"], false);
    assert!(fetch["result"]["structuredContent"]["error"].is_null());
}
