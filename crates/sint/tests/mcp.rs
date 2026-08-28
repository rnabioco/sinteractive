//! `sinteractive mcp` against the fake Slurm: JSON-RPC spoken by hand over
//! the server's stdin/stdout, so the wire format is what a real client
//! sees and nothing but JSON-RPC ever reaches stdout.

mod common;

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::Duration;

use common::{FakeSlurm, Job};
use serde_json::{json, Value};

/// The server process with a line-oriented JSON-RPC client around it.
struct Mcp {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Mcp {
    fn spawn(fx: &FakeSlurm, extra_env: &[(&str, String)]) -> Self {
        let exe = assert_cmd::cargo::cargo_bin("sinteractive");
        let mut cmd = Command::new(exe);
        cmd.args(["claude", "mcp"])
            .env_clear()
            .env("PATH", fx.path())
            .env("HOME", fx.home_dir())
            .env("USER", "tester")
            .env("FAKE_SLURM_DIR", fx.dir())
            .env("SINTERACTIVE_CACHE", fx.cache_dir())
            .env("CLAUDE_CONFIG_DIR", fx.claude_dir())
            .env("SINTERACTIVE_COLOR", "never")
            .current_dir(fx.home_dir())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().expect("spawn sinteractive mcp");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Mcp {
            child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    fn send(&mut self, msg: Value) {
        let mut line = msg.to_string();
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).unwrap();
        self.stdin.flush().unwrap();
    }

    /// The next stdout line, which must be one JSON-RPC message.
    fn recv(&mut self) -> Value {
        let mut line = String::new();
        let n = self.stdout.read_line(&mut line).expect("read stdout");
        assert!(n > 0, "server closed stdout");
        let v: Value = serde_json::from_str(line.trim_end())
            .unwrap_or_else(|e| panic!("stdout is not JSON-RPC ({e}): {line:?}"));
        assert_eq!(v["jsonrpc"], "2.0", "not a JSON-RPC message: {line}");
        v
    }

    /// Send a request and return its response's `result`.
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let v = self.recv();
            // Notifications from the server (none expected) would have no id.
            if v["id"] == json!(id) {
                assert!(v.get("error").is_none(), "{method} failed: {}", v["error"]);
                return v["result"].clone();
            }
        }
    }

    fn initialize(&mut self) -> Value {
        let result = self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "0"}
            }),
        );
        self.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        result
    }

    fn call(&mut self, tool: &str, args: Value) -> Value {
        self.request("tools/call", json!({"name": tool, "arguments": args}))
    }

    /// Close stdin, wait for the server to exit, and return its stderr.
    fn finish(mut self) -> String {
        drop(self.stdin);
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(_status) = self.child.try_wait().unwrap() {
                break;
            }
            if std::time::Instant::now() > deadline {
                let _ = self.child.kill();
                panic!("server did not exit after stdin closed");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        // Anything left on stdout must still be JSON-RPC.
        let mut rest = String::new();
        std::io::Read::read_to_string(&mut self.stdout, &mut rest).unwrap();
        for line in rest.lines().filter(|l| !l.trim().is_empty()) {
            let v: Value = serde_json::from_str(line).expect("trailing stdout is JSON-RPC");
            assert_eq!(v["jsonrpc"], "2.0");
        }
        let mut err = String::new();
        std::io::Read::read_to_string(self.child.stderr.as_mut().unwrap(), &mut err).unwrap();
        err
    }
}

/// The JSON a tool call returned: structured content when it has any, else
/// its (single) text block parsed.
fn payload(result: &Value) -> Value {
    if let Some(s) = result.get("structuredContent") {
        return s.clone();
    }
    let text = result["content"][0]["text"].as_str().expect("text content");
    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_string()))
}

fn seeded() -> FakeSlurm {
    FakeSlurm::with_jobs(&[
        Job::new(147845, "sinteractive:web"),
        Job::new(147900, "cargo-ci"),
    ])
}

#[test]
fn handshake_tools_and_the_documented_contracts() {
    let fx = seeded();
    let mut mcp = Mcp::spawn(&fx, &[]);

    let init = mcp.initialize();
    assert_eq!(init["serverInfo"]["name"], "sinteractive");
    assert_eq!(init["serverInfo"]["version"], env!("CARGO_PKG_VERSION"));
    assert!(init["capabilities"]["tools"].is_object());
    assert!(init["capabilities"]["resources"].is_object());
    let instructions = init["instructions"].as_str().unwrap();
    assert!(
        instructions.contains("not a compute target"),
        "{instructions}"
    );
    assert!(instructions.contains("wait_for_event"), "{instructions}");

    let tools = mcp.request("tools/list", json!({}));
    let mut names: Vec<&str> = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "agent_context",
            "cancel_session",
            "ensure_session",
            "list_sessions",
            "monitor_snapshot",
            "peek",
            "queue",
            "quota",
            "send",
            "session_status",
            "wait_for_event",
        ]
    );
    // Every tool has an object input schema; the JSON-returning ones an
    // output schema that mirrors the CLI struct.
    for t in tools["tools"].as_array().unwrap() {
        assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
        assert!(t["description"].as_str().is_some_and(|d| !d.is_empty()));
    }
    let status_tool = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "session_status")
        .unwrap();
    let props = &status_tool["outputSchema"]["properties"];
    for key in [
        "job_id",
        "name",
        "state",
        "node",
        "remaining_seconds",
        "gpus",
    ] {
        assert!(
            props.get(key).is_some(),
            "session_status output schema lacks {key}"
        );
    }
    assert!(status_tool["inputSchema"]["properties"]["target"].is_object());

    // list_sessions: the seeded session with the documented fields; the
    // foreign job is not a session.
    let r = mcp.call("list_sessions", json!({}));
    assert_ne!(r["isError"], json!(true), "{r}");
    let sessions = payload(&r)["sessions"].clone();
    let rows = sessions.as_array().expect("array");
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row["job_id"], 147845);
    assert_eq!(row["name"], "web");
    assert_eq!(row["state"], "RUNNING");
    assert_eq!(row["node"], "node01");
    assert_eq!(row["partition"], "interactive");
    assert_eq!(row["cpus"], 4);
    assert_eq!(row["memory"], "16G");
    assert_eq!(row["memory_mb"], 16384);
    assert_eq!(row["gpus"], 0);
    assert_eq!(row["time_limit"], "8:00:00");
    assert_eq!(row["elapsed"], "1:02:03");
    assert!(row.get("end_epoch").is_some());
    assert!(row.get("remaining_seconds").is_some());
    assert_eq!(row["cwd"], Value::Null);
    // Same key order as `list --json`, cwd last.
    let keys: Vec<&str> = row
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys.first(), Some(&"job_id"));
    assert_eq!(keys.last(), Some(&"cwd"));

    // session_status by name.
    let r = mcp.call("session_status", json!({"target": "web"}));
    assert_ne!(r["isError"], json!(true), "{r}");
    let info = payload(&r);
    assert_eq!(info["job_id"], 147845);
    assert_eq!(info["name"], "web");
    assert_eq!(info["state"], "RUNNING");
    assert!(info.get("cwd").is_none(), "status carries no cwd: {info}");
    // The text block carries the same JSON for clients without structured
    // content support.
    let text: Value = serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text, info);

    // Unknown name: an error result in the CLI's wording, not a protocol error.
    let r = mcp.call("session_status", json!({"target": "nope"}));
    assert_eq!(r["isError"], true);
    assert!(
        r["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("no sinteractive session named 'nope'"),
        "{r}"
    );
    // Gone job: the NOT_FOUND object.
    let r = mcp.call("session_status", json!({"target": "999"}));
    assert_eq!(r["isError"], true);
    assert_eq!(payload(&r), json!({"job_id": 999, "state": "NOT_FOUND"}));
    // No target outside a session.
    let r = mcp.call("session_status", json!({}));
    assert_eq!(r["isError"], true);
    assert!(r["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("not inside an sinteractive session"));

    // cancel_session.
    let r = mcp.call("cancel_session", json!({"target": "web"}));
    assert_ne!(r["isError"], json!(true), "{r}");
    assert_eq!(payload(&r), json!({"job_id": 147845, "cancelled": true}));
    assert_eq!(fx.calls_to("scancel"), vec![vec!["147845".to_string()]]);
    let r = mcp.call("list_sessions", json!({}));
    assert_eq!(payload(&r), json!({"sessions": []}));
    let r = mcp.call("cancel_session", json!({"target": "424242"}));
    assert_eq!(r["isError"], true);
    let p = payload(&r);
    assert_eq!(p["job_id"], 424242);
    assert_eq!(p["cancelled"], false);
    assert!(p["error"]
        .as_str()
        .unwrap()
        .contains("could not cancel job 424242"));

    let stderr = mcp.finish();
    // The CLI's stderr narration for these tools is empty; nothing useful
    // to assert beyond the server having exited cleanly.
    let _ = stderr;
}

#[test]
fn queue_quota_context_and_snapshot() {
    let fx = seeded();
    fx.write(
        "sacct",
        "31757001|make-test|rna|COMPLETED|00:10:00|32G|1234K|8|2026-01-01T01:00:00\n",
    );
    let mut mcp = Mcp::spawn(&fx, &[("SINTERACTIVE_JOB_ID", "147845".to_string())]);
    mcp.initialize();

    let r = mcp.call("queue", json!({}));
    assert_ne!(r["isError"], json!(true), "{r}");
    let q = payload(&r);
    assert_eq!(q["running"].as_array().unwrap().len(), 2);
    assert_eq!(q["running"][0]["job_id"], 147845);
    assert_eq!(q["running"][0]["job_name"], "sint-web");
    assert!(q["running"][0].get("reason").is_none());
    assert_eq!(q["pending"], json!([]));
    assert_eq!(q["partitions"], json!([]));
    assert!(q["recent"].is_array());
    let r = mcp.call("queue", json!({"all": true}));
    let q = payload(&r);
    assert_eq!(q["partitions"][0]["partition"], "interactive");
    assert_eq!(q["partitions"][0]["running"], 2);

    // quota: nothing cached.
    let r = mcp.call("quota", json!({}));
    assert_eq!(r["isError"], true);
    assert_eq!(payload(&r), json!({"error": "quota unavailable"}));
    std::fs::write(
        fx.cache_dir().join("quota.json"),
        "{\"user\":\"tester\",\"used_kb\":537185280,\"hard_kb\":524288000,\"over_kb\":12897280,\"pct\":102,\"over\":true,\"checked_epoch\":1}\n",
    )
    .unwrap();
    let r = mcp.call("quota", json!({}));
    assert_ne!(r["isError"], json!(true), "{r}");
    let q = payload(&r);
    assert_eq!(q["user"], "tester");
    assert_eq!(q["over"], true);
    assert_eq!(q["pct"], 102);

    // agent_context, inside the seeded session.
    let r = mcp.call("agent_context", json!({}));
    assert_ne!(r["isError"], json!(true), "{r}");
    let text = payload(&r)["text"].as_str().unwrap().to_string();
    assert!(text.starts_with("You are inside an sinteractive"), "{text}");
    assert!(text.contains("job 147845 (web) on node01"), "{text}");
    assert!(text.contains("OVER STORAGE QUOTA"), "{text}");

    // session_status with no target: the current session.
    let r = mcp.call("session_status", json!({}));
    assert_eq!(payload(&r)["job_id"], 147845);

    // monitor_snapshot: nothing yet, then the file verbatim.
    let r = mcp.call("monitor_snapshot", json!({}));
    assert_eq!(r["isError"], true);
    assert_eq!(
        payload(&r),
        json!({"job_id": 147845, "error": "no snapshot yet"})
    );
    std::fs::write(
        fx.cache_dir().join("147845.metrics.json"),
        "{\"ts\":5,\"cpu\":{\"pct\":12.5},\"gpus\":[]}\n",
    )
    .unwrap();
    let r = mcp.call("monitor_snapshot", json!({"target": "web"}));
    assert_ne!(r["isError"], json!(true), "{r}");
    assert_eq!(
        payload(&r),
        json!({"ts": 5, "cpu": {"pct": 12.5}, "gpus": []})
    );

    // wait_for_event: a line appended to the log after the call started.
    let log = fx.cache_dir().join("147845.events.ndjson");
    std::fs::write(&log, "{\"ts\":1,\"kind\":\"old\"}\n").unwrap();
    let writer = {
        let log = log.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1500));
            let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
            f.write_all(b"{\"ts\":2,\"kind\":\"gpu_idle\",\"minutes\":15}\n")
                .unwrap();
        })
    };
    let r = mcp.call(
        "wait_for_event",
        json!({"kinds": ["gpu_idle"], "timeout_secs": 30}),
    );
    writer.join().unwrap();
    assert_ne!(r["isError"], json!(true), "{r}");
    assert_eq!(
        payload(&r),
        json!({"ts": 2, "kind": "gpu_idle", "minutes": 15})
    );
    let r = mcp.call("wait_for_event", json!({"timeout_secs": 0}));
    assert_eq!(payload(&r), json!({"timed_out": true}));

    // Resources: the list and the per-session trio.
    let r = mcp.request("resources/list", json!({}));
    let uris: Vec<&str> = r["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["uri"].as_str().unwrap())
        .collect();
    assert!(uris.contains(&"sinteractive://sessions"), "{uris:?}");
    assert!(
        uris.contains(&"sinteractive://sessions/147845/status"),
        "{uris:?}"
    );
    assert!(
        uris.contains(&"sinteractive://sessions/147845/notices"),
        "{uris:?}"
    );
    assert!(
        uris.contains(&"sinteractive://sessions/147845/metrics"),
        "{uris:?}"
    );
    let r = mcp.request("resources/templates/list", json!({}));
    assert_eq!(r["resourceTemplates"].as_array().unwrap().len(), 3);
    let r = mcp.request(
        "resources/read",
        json!({"uri": "sinteractive://sessions/147845/status"}),
    );
    let text: Value = serde_json::from_str(r["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text["job_id"], 147845);
    assert_eq!(r["contents"][0]["mimeType"], "application/json");
    let r = mcp.request("resources/read", json!({"uri": "sinteractive://sessions"}));
    let text: Value = serde_json::from_str(r["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text.as_array().unwrap().len(), 1);
    let r = mcp.request(
        "resources/read",
        json!({"uri": "sinteractive://sessions/147845/metrics"}),
    );
    let text: Value = serde_json::from_str(r["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text["ts"], 5);

    mcp.finish();
}

#[test]
fn peek_and_send_go_over_ssh_and_report_failures() {
    let fx = FakeSlurm::with_jobs(&[
        Job::new(147845, "sinteractive:web"),
        Job::new(147900, "sinteractive:queued")
            .state("PENDING")
            .node("")
            .reason("Priority"),
    ]);
    let runtime = fx.tmp.path().join("runtime");
    let mut mcp = Mcp::spawn(
        &fx,
        &[("SINTERACTIVE_RUNTIME_DIR", runtime.display().to_string())],
    );
    mcp.initialize();

    // No zellij server behind the fake ssh, so the hop fails the way it
    // does when a session's server has gone: an error result, one ssh call.
    let r = mcp.call("peek", json!({"target": "web", "lines": 5}));
    assert_eq!(r["isError"], true, "{r}");
    assert!(r["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("could not read the screen of session 147845 on node01"));
    let calls = fx.calls_to("ssh");
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(calls[0]
        .last()
        .unwrap()
        .contains("zellij action dump-screen -p terminal_0 --full"));

    let r = mcp.call("send", json!({"target": "web", "command": "ls"}));
    assert_eq!(r["isError"], true, "{r}");
    assert!(r["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("could not send to session 147845 on node01"));
    assert_eq!(fx.calls_to("ssh").len(), 2);

    // Not running: refused before any ssh.
    let r = mcp.call("peek", json!({"target": "queued"}));
    assert_eq!(r["isError"], true);
    assert!(r["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("session 147900 is not running (state: PENDING)"));
    let r = mcp.call("send", json!({"target": "web", "command": "  "}));
    assert_eq!(r["isError"], true);
    assert!(r["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("command is empty"));
    assert_eq!(fx.calls_to("ssh").len(), 2);

    mcp.finish();
}

#[test]
fn ensure_session_launches_then_reuses_with_a_clean_stdout() {
    let fx = FakeSlurm::new();
    let runtime = fx.tmp.path().join("runtime");
    let ready = runtime.join("sint-1000");
    std::fs::create_dir_all(&ready).unwrap();
    std::fs::write(ready.join("ready"), "").unwrap();
    let mut mcp = Mcp::spawn(
        &fx,
        &[
            ("SINTERACTIVE_RUNTIME_DIR", runtime.display().to_string()),
            ("SINTERACTIVE_POLL_FAST", "0".to_string()),
        ],
    );
    mcp.initialize();

    let r = mcp.call(
        "ensure_session",
        json!({"name": "web", "time": "2h", "cpus": 3, "mem": "12G", "partition": "rna"}),
    );
    assert_ne!(r["isError"], json!(true), "{r}");
    let info = payload(&r);
    assert_eq!(info["created"], true);
    assert_eq!(info["job_id"], 1000);
    assert_eq!(info["name"], "web");
    assert_eq!(info["state"], "RUNNING");
    assert_eq!(info["partition"], "rna");
    assert_eq!(info["cpus"], 3);
    assert_eq!(info["memory"], "12G");
    assert_eq!(info["time_limit"], "02:00:00");

    let r = mcp.call("ensure_session", json!({"name": "web"}));
    assert_ne!(r["isError"], json!(true), "{r}");
    let info = payload(&r);
    assert_eq!(info["created"], false);
    assert_eq!(info["job_id"], 1000);
    assert_eq!(fx.calls_to("sbatch").len(), 1, "launched once");

    // A bad name is refused without a submission.
    let r = mcp.call("ensure_session", json!({"name": "no/slash"}));
    assert_eq!(r["isError"], true, "{r}");
    assert_eq!(payload(&r)["error"], "launch failed");
    assert_eq!(fx.calls_to("sbatch").len(), 1);

    // The launch narrated on stderr; stdout was JSON-RPC only (recv()
    // asserted every line on the way through).
    let stderr = mcp.finish();
    assert!(stderr.contains("Submitted job"), "{stderr}");
    assert!(
        stderr.contains("is ready on") || stderr.contains("bringing up"),
        "{stderr}"
    );
}
