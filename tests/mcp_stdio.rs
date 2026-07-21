use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

use serde_json::{Value, json};

struct McpProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpProcess {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_secretbroker"))
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("start MCP server");
        let stdin = child.stdin.take().expect("MCP stdin");
        let stdout = BufReader::new(child.stdout.take().expect("MCP stdout"));
        Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    fn initialize(&mut self) {
        let response = self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "secretbroker-test", "version": "1"}
            }),
        );
        assert_eq!(response["result"]["serverInfo"]["name"], "secretbroker");
        self.notify("notifications/initialized", None);
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        writeln!(
            self.stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
        )
        .expect("write MCP request");
        self.stdin.flush().expect("flush MCP request");
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("read MCP response");
        let response: Value = serde_json::from_str(&line).expect("valid MCP response");
        assert_eq!(response["id"], id);
        response
    }

    fn notify(&mut self, method: &str, params: Option<Value>) {
        let mut notification = json!({"jsonrpc": "2.0", "method": method});
        if let Some(params) = params {
            notification["params"] = params;
        }
        writeln!(self.stdin, "{notification}").expect("write MCP notification");
        self.stdin.flush().expect("flush MCP notification");
    }
}

impl Drop for McpProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn exposes_metadata_only_tools_and_widget_over_stdio() {
    let mut server = McpProcess::start();
    server.initialize();

    let tools = server.request("tools/list", json!({}));
    let tools = tools["result"]["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 2);
    let open = tools
        .iter()
        .find(|tool| tool["name"] == "secretbroker_open")
        .expect("open tool");
    assert_eq!(
        open["_meta"]["openai/outputTemplate"],
        "ui://secretbroker/request-status.html"
    );
    assert!(!open.to_string().contains("http://"));
    let status = tools
        .iter()
        .find(|tool| tool["name"] == "secretbroker_status")
        .expect("status tool");
    assert_eq!(status["_meta"]["ui"]["visibility"], json!(["app"]));

    let resource = server.request(
        "resources/read",
        json!({"uri": "ui://secretbroker/request-status.html"}),
    );
    let contents = &resource["result"]["contents"][0];
    assert_eq!(contents["mimeType"], "text/html;profile=mcp-app");
    let html = contents["text"].as_str().expect("widget HTML");
    assert!(!html.contains("<input"));
    assert!(!html.contains("type=\"password\""));

    let result = server.request(
        "tools/call",
        json!({
            "name": "secretbroker_status",
            "arguments": {
                "scope": "session:mcp-stdio-test",
                "variables": ["SECRETBROKER_TEST_VALUE_NEVER_SET"]
            }
        }),
    );
    assert_eq!(
        result["result"]["structuredContent"]["missing"],
        json!(["SECRETBROKER_TEST_VALUE_NEVER_SET"])
    );
    assert_eq!(
        result["result"]["structuredContent"]["launchStatus"],
        "idle"
    );
    assert!(result["result"].get("_meta").is_none());
}
