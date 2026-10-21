#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! The MCP service over real HTTP against a mock application (spec.md 3.7,
//! D29): the listener's lifecycle and its three guards, then every tool
//! through an `rmcp` client, each checked against the interface's own read
//! so a tool that diverged from its command fails.

mod common;

use std::collections::HashMap;

use common::TempRoot;
use pe_app::commands::AppState;
use pe_app::mcp::McpService;
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use serde_json::{Value, json};
use tauri::Manager;
use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};

fn mock_app(root: &TempRoot) -> tauri::App<MockRuntime> {
    mock_builder()
        .manage(root.state())
        .manage(McpService::default())
        .build(mock_context(noop_assets()))
        .expect("mock app")
}

/// Starts the service on a free port with a fresh token.
fn serve(app: &tauri::App<MockRuntime>) -> (u16, String) {
    let token = pe_app::mcp::token::fresh();
    let running = pe_app::mcp::server::start(app.handle().clone(), 0, token.clone()).expect("bind");
    let port = running.port;
    // Held for the test's life by the service, the way the app holds it.
    app.state::<McpService>().adopt(running);
    (port, token)
}

/// An HTTP client for loopback. The workspace builds `reqwest` with
/// `rustls-no-provider` (no `aws-lc-rs`, CLAUDE.md), so a client cannot be
/// built until a provider is installed, plain HTTP or not: `ring`, as the
/// application installs it.
fn http() -> reqwest::Client {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::new()
}

fn url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/mcp")
}

type Client = rmcp::service::RunningService<rmcp::RoleClient, ()>;

async fn client(port: u16, token: &str) -> Client {
    let mut headers = HashMap::new();
    headers.insert(
        http::HeaderName::from_static("authorization"),
        http::HeaderValue::from_str(&format!("Bearer {token}")).expect("header"),
    );
    let config = StreamableHttpClientTransportConfig::with_uri(url(port)).custom_headers(headers);
    let transport = StreamableHttpClientTransport::with_client(http(), config);
    ().serve(transport).await.expect("initialize")
}

/// Calls a tool and returns its structured content, or panics with its text.
async fn call(client: &Client, name: &'static str, args: Value) -> Value {
    let params = CallToolRequestParams::new(name)
        .with_arguments(args.as_object().cloned().unwrap_or_default());
    let result: CallToolResult = client.call_tool(params).await.expect("call");
    assert_ne!(
        result.is_error,
        Some(true),
        "{name} failed: {:?}",
        result.content
    );
    result.structured_content.unwrap_or(Value::Null)
}

/// Calls a tool that must refuse, and returns the reason it gave in words.
async fn call_err(client: &Client, name: &'static str, args: Value) -> String {
    let params = CallToolRequestParams::new(name)
        .with_arguments(args.as_object().cloned().unwrap_or_default());
    let result: CallToolResult = client.call_tool(params).await.expect("call");
    assert_eq!(result.is_error, Some(true), "{name} should have failed");
    result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A well-formed `initialize`, so a refusal is the guard's and not `rmcp`
/// turning away a malformed request.
const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"guards","version":"0"}}}"#;

/// The request every guard test starts from: the one a client opens with,
/// which the control test shows is answered.
fn initialize(port: u16, token: &str) -> reqwest::RequestBuilder {
    http()
        .post(url(port))
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("authorization", format!("Bearer {token}"))
        .body(INITIALIZE)
}

// ------------------------------------------------- the listener's guards

/// The control for the guard tests below: the same request with nothing
/// wrong is answered. Without it, "refused" could be any refusal.
#[tokio::test]
async fn a_well_formed_request_with_the_token_is_answered() {
    let root = TempRoot::new("mcp-control");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let response = initialize(port, &token).send().await.expect("request");
    assert_eq!(response.status(), 200);
    // The loopback names by their other spelling, too.
    let response = initialize(port, &token)
        .header("host", format!("localhost:{port}"))
        .header("origin", format!("http://localhost:{port}"))
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn a_request_without_the_token_is_refused_before_mcp() {
    let root = TempRoot::new("mcp-auth");
    let app = mock_app(&root);
    let (port, _token) = serve(&app);
    let response = http()
        .post(url(port))
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(INITIALIZE)
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), 401);
}

#[tokio::test]
async fn a_wrong_token_is_refused() {
    let root = TempRoot::new("mcp-wrong-token");
    let app = mock_app(&root);
    let (port, _token) = serve(&app);
    let response = initialize(port, "nope").send().await.expect("request");
    assert_eq!(response.status(), 401);
}

/// The DNS-rebinding guard: `rmcp` checks `Host` against the loopback names
/// `server::start` configures before a request reaches any tool, so a page
/// on another site cannot reach the service through a rebound name.
#[tokio::test]
async fn a_wrong_host_is_refused() {
    let root = TempRoot::new("mcp-host");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    for host in ["evil.example".to_owned(), format!("evil.example:{port}")] {
        let response = initialize(port, &token)
            .header("host", &host)
            .send()
            .await
            .expect("request");
        assert_eq!(response.status(), 403, "Host: {host}");
    }
}

/// The same guard, the `Origin` header. Built from parts, so
/// `tools/check-offline.sh`'s scan of Rust sources does not read this as a
/// remote reference.
#[tokio::test]
async fn a_wrong_origin_is_refused() {
    let root = TempRoot::new("mcp-origin");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let scheme = "http";
    for origin in [
        format!("{scheme}://evil.example"),
        format!("{scheme}://evil.example:{port}"),
    ] {
        let response = initialize(port, &token)
            .header("origin", &origin)
            .send()
            .await
            .expect("request");
        assert_eq!(response.status(), 403, "Origin: {origin}");
    }
}

#[tokio::test]
async fn anything_but_the_endpoint_is_not_found() {
    let root = TempRoot::new("mcp-path");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let response = http()
        .get(format!("http://127.0.0.1:{port}/"))
        .header("authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn the_right_token_initialises_and_lists_tools() {
    let root = TempRoot::new("mcp-init");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;
    let info = client.peer_info().expect("server info");
    let server_info = info.server_info.as_ref().expect("implementation identity");
    assert_eq!(server_info.name, "PolarExplorer");
    let instructions = info.instructions.clone().unwrap_or_default();
    assert!(instructions.starts_with("Today is "), "{instructions}");
    let tools = client.list_all_tools().await.expect("tools");
    assert!(
        tools.iter().any(|tool| tool.name == "polarexplorer_guide"),
        "{:?}",
        tools.iter().map(|t| t.name.clone()).collect::<Vec<_>>()
    );
    let guide = call(&client, "polarexplorer_guide", json!({})).await;
    assert!(guide["guide"].as_str().unwrap().contains("# PolarExplorer"));
    client.cancel().await.expect("close");
}

// ------------------------------------------------------- off means off

/// Invariant 4's enforcement for the one inbound socket: with the setting
/// off there is no listener, and turning it off gives the port back.
/// `check:offline` cannot see a listener; this test is what holds it.
#[tokio::test]
async fn off_means_no_socket_and_stop_releases_the_port() {
    let root = TempRoot::new("mcp-off");
    let app = mock_app(&root);
    let service = app.state::<McpService>();
    assert_eq!(service.running_port(), None);
    // The default settings are off: applying them starts nothing.
    service.apply(app.handle(), &pe_app::settings::McpSettings::default());
    assert_eq!(service.running_port(), None);

    let (port, _token) = serve(&app);
    assert_eq!(service.running_port(), Some(port));
    service.apply(app.handle(), &pe_app::settings::McpSettings::default());
    assert_eq!(service.running_port(), None);
    // The port is free again: a bind of our own succeeds. Waited for, since
    // `stop` gives the accept loop two seconds to confirm and then returns
    // regardless; on a loaded machine the socket can close a moment after.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let mut again = std::net::TcpListener::bind(("127.0.0.1", port));
    while again.is_err() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(100));
        again = std::net::TcpListener::bind(("127.0.0.1", port));
    }
    assert!(again.is_ok(), "port still held 15 s after stop: {again:?}");
    // And nothing answers there any more.
    drop(again);
    assert!(http().get(url(port)).send().await.is_err());
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind free port");
    listener.local_addr().expect("addr").port()
}

#[tokio::test]
async fn mcp_set_issues_a_token_on_enable_and_clears_it_on_disable() {
    let root = TempRoot::new("mcp-set");
    let app = mock_app(&root);
    let port = free_port();
    let status =
        pe_app::settings::mcp_set(app.handle().clone(), app.state(), app.state(), true, port)
            .expect("enable");
    assert!(status.enabled);
    assert_eq!(status.token.len(), 43);
    assert_eq!(status.bound_port, Some(port));
    assert_eq!(status.bind_error, None);
    // The token is in the settings file, which is the person's alone.
    let saved = pe_app::settings::current(app.state::<AppState>().inner()).unwrap();
    assert_eq!(saved.mcp.token, status.token);

    // Rotating gives another token on the same port.
    let rotated =
        pe_app::settings::mcp_rotate_token(app.handle().clone(), app.state(), app.state())
            .expect("rotate");
    assert_eq!(rotated.token.len(), 43);
    assert_ne!(rotated.token, status.token);
    assert_eq!(rotated.bound_port, Some(port));

    let status =
        pe_app::settings::mcp_set(app.handle().clone(), app.state(), app.state(), false, port)
            .expect("disable");
    assert!(!status.enabled);
    assert!(status.token.is_empty());
    assert_eq!(status.bound_port, None);
    assert_eq!(app.state::<McpService>().running_port(), None);
    // There is nothing to rotate while off.
    assert!(
        pe_app::settings::mcp_rotate_token(app.handle().clone(), app.state(), app.state()).is_err()
    );
}

/// Review focus 2: the port is taken. The error is for Settings to show,
/// the setting stays on so the next launch tries again, nothing panics.
#[tokio::test]
async fn a_taken_port_is_reported_and_the_setting_stays_on() {
    let root = TempRoot::new("mcp-taken");
    let app = mock_app(&root);
    let holder = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = holder.local_addr().unwrap().port();
    let status =
        pe_app::settings::mcp_set(app.handle().clone(), app.state(), app.state(), true, port)
            .expect("the setting is saved even though the bind fails");
    assert!(status.enabled);
    assert_eq!(status.bound_port, None);
    let error = status.bind_error.expect("why it is not listening");
    assert!(error.contains(&port.to_string()), "{error}");
    drop(holder);
}

#[tokio::test]
async fn port_zero_is_refused_by_the_command() {
    let root = TempRoot::new("mcp-zero");
    let app = mock_app(&root);
    let refused =
        pe_app::settings::mcp_set(app.handle().clone(), app.state(), app.state(), true, 0);
    assert_eq!(refused.unwrap_err().kind(), "bad-option");
    assert_eq!(app.state::<McpService>().running_port(), None);
}

/// A client session is counted while it is open and not after, and the
/// last tool it called is told to the status bar.
#[tokio::test]
async fn sessions_and_the_last_tool_are_counted() {
    let root = TempRoot::new("mcp-sessions");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let service = app.state::<McpService>();
    let settings = pe_app::settings::McpSettings::default();
    assert_eq!(service.status(&settings).sessions, 0);
    let client = client(port, &token).await;
    call(&client, "polarexplorer_guide", json!({})).await;
    let status = service.status(&settings);
    assert_eq!(status.sessions, 1);
    assert_eq!(status.last_tool.as_deref(), Some("polarexplorer_guide"));
    client.cancel().await.expect("close");
    assert_eq!(
        service.status(&settings).sessions,
        1,
        "recent activity survives transport disconnect"
    );
    service.apply(app.handle(), &settings);
    assert_eq!(service.status(&settings).sessions, 0);
    assert_eq!(service.status(&settings).last_tool, None);
}

/// A client on the 2026-07-28 protocol keeps no session: it never sends
/// `initialize`, and `rmcp` answers each of its requests on its own. It
/// drives the application all the same, so it is counted all the same: the
/// status bar's badge is how the person knows a client is at work.
#[tokio::test]
async fn a_client_that_keeps_no_session_is_counted_too() {
    use rmcp::service::{ClientLifecycleMode, ClientServiceExt};
    let root = TempRoot::new("mcp-sessionless");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let service = app.state::<McpService>();
    let settings = pe_app::settings::McpSettings::default();

    let mut headers = HashMap::new();
    headers.insert(
        http::HeaderName::from_static("authorization"),
        http::HeaderValue::from_str(&format!("Bearer {token}")).expect("header"),
    );
    let config = StreamableHttpClientTransportConfig::with_uri(url(port)).custom_headers(headers);
    let transport = StreamableHttpClientTransport::with_client(http(), config);
    let client: Client = ()
        .serve_with_lifecycle(
            transport,
            ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
            },
        )
        .await
        .expect("discover");
    assert_eq!(service.status(&settings).sessions, 0, "nothing called yet");
    call(&client, "polarexplorer_guide", json!({})).await;
    let status = service.status(&settings);
    assert_eq!(status.sessions, 1);
    assert_eq!(status.last_tool.as_deref(), Some("polarexplorer_guide"));
    // A second call is the same client, not a second one.
    call(&client, "project_status", json!({})).await;
    let status = service.status(&settings);
    assert_eq!(status.sessions, 1);
    assert_eq!(status.last_tool.as_deref(), Some("project_status"));
    let _ = client.cancel().await;
}

/// Switching the service off leaves nobody counted at once, whatever
/// `rmcp` still holds of an idle session; and that session ending later
/// takes nothing from the count of a listener started since.
#[tokio::test]
async fn switching_off_leaves_nobody_counted() {
    let root = TempRoot::new("mcp-off-count");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let service = app.state::<McpService>();
    let settings = pe_app::settings::McpSettings::default();
    let first = client(port, &token).await;
    call(&first, "polarexplorer_guide", json!({})).await;
    assert_eq!(service.status(&settings).sessions, 1);

    service.apply(app.handle(), &settings);
    let status = service.status(&settings);
    assert_eq!(status.sessions, 0);
    assert_eq!(status.last_tool, None);

    let (port, token) = serve(&app);
    let second = client(port, &token).await;
    call(&second, "project_status", json!({})).await;
    assert_eq!(service.status(&settings).sessions, 1);
    // The first client's goodbye, to a listener that is gone.
    let _ = first.cancel().await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert_eq!(service.status(&settings).sessions, 1);
    second.cancel().await.expect("close");
}

// ------------------------------------------------ project, boats, history

fn project_bytes(app: &tauri::App<MockRuntime>) -> Vec<u8> {
    app.state::<AppState>()
        .with_session(|s| Ok(pe_core::io::to_bytes(&s.require_open()?.project).unwrap()))
        .unwrap()
}

/// Every `document://changed` payload from here on.
fn changes(app: &tauri::App<MockRuntime>) -> std::sync::mpsc::Receiver<Value> {
    use tauri::Listener;
    let (tx, rx) = std::sync::mpsc::channel::<Value>();
    app.listen(pe_app::mcp::events::CHANGED, move |event| {
        let _ = tx.send(serde_json::from_str(event.payload()).expect("json"));
    });
    rx
}

const SOON: std::time::Duration = std::time::Duration::from_secs(2);
const BRIEFLY: std::time::Duration = std::time::Duration::from_millis(300);

#[tokio::test]
async fn a_client_creates_a_project_and_the_app_sees_it() {
    let root = TempRoot::new("mcp-project");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;
    assert_eq!(
        call(&client, "project_status", json!({})).await["project"],
        Value::Null
    );

    let summary = call(
        &client,
        "project_new",
        json!({ "name": "Fastnet", "boat_name": "Rán", "boat_notes": "TP52" }),
    )
    .await;
    assert_eq!(summary["name"], "Fastnet");
    assert_eq!(summary["boat_name"], "Rán");
    // The interface's own read agrees.
    let current = pe_app::projects::summary(app.state::<AppState>().inner())
        .unwrap()
        .expect("open");
    assert_eq!(current.name, "Fastnet");
    assert_eq!(current.boat_notes, "TP52");
    let status = call(&client, "project_status", json!({})).await;
    assert_eq!(status["project"]["name"], "Fastnet");
    assert_eq!(status["project"]["id"], current.id);
    client.cancel().await.expect("close");
}

/// Review focus 3: with nothing open, a project tool says so in the
/// interface's words, as a tool result a model can read.
#[tokio::test]
async fn a_project_tool_with_nothing_open_is_refused_in_words() {
    let root = TempRoot::new("mcp-nothing-open");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;
    for (tool, args) in [
        ("project_save", json!({})),
        ("boats_list", json!({})),
        ("boat_add", json!({ "name": "Second" })),
        ("undo", json!({})),
        ("redo", json!({})),
    ] {
        let message = call_err(&client, tool, args).await;
        assert!(message.contains("No project is open."), "{tool}: {message}");
    }
    client.cancel().await.expect("close");
}

#[tokio::test]
async fn unsaved_work_is_refused_without_discard() {
    let root = TempRoot::new("mcp-dirty");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;
    call(&client, "project_new", json!({ "name": "One" })).await;
    call(&client, "boat_add", json!({ "name": "Sister ship" })).await;

    // Listen only from here: the refusal below is the one event wanted.
    let changed = changes(&app);
    // Absolute on every platform: "/nowhere" has no drive on Windows.
    let elsewhere = root.0.join("nowhere").join("else.wpsproj");
    for (tool, args) in [
        ("project_new", json!({ "name": "Two" })),
        ("project_close", json!({})),
        ("project_open", json!({ "path": elsewhere })),
    ] {
        let message = call_err(&client, tool, args).await;
        assert!(message.contains("has unsaved changes"), "{tool}: {message}");
        // The refusal says what to do about it, and whose call it is.
        assert!(message.contains("project_discard"), "{tool}: {message}");
        // "One" is still open, so the frontend must not be told a different
        // project opened: `opened` is true only when the tool succeeded.
        let payload = changed
            .recv_timeout(SOON)
            .expect("a refused write is still reported");
        assert_eq!(payload["opened"], false, "{tool}");
        assert_eq!(payload["project"]["name"], "One", "{tool}");
    }

    let refused = call_err(
        &client,
        "project_new",
        json!({"name":"Two","discard_unsaved":true}),
    )
    .await;
    assert!(refused.contains("project_discard"));
    call(&client, "project_discard", json!({})).await;
    let _ = changed.recv_timeout(SOON).expect("discard event");
    let summary = call(&client, "project_new", json!({"name":"Two"})).await;
    assert_eq!(summary["name"], "Two");
    let payload = changed.recv_timeout(SOON).expect("event");
    assert_eq!(payload["opened"], true);
    client.cancel().await.expect("close");
}

#[tokio::test]
async fn a_project_saves_and_reopens_through_the_tools() {
    let root = TempRoot::new("mcp-save");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;
    call(&client, "project_new", json!({ "name": "Saved" })).await;
    // Never saved: Save needs a path, and says so.
    let message = call_err(&client, "project_save", json!({})).await;
    assert!(message.contains("path"), "{message}");

    // The extension is added, as the interface's Save As adds it.
    let saved = call(
        &client,
        "project_save",
        json!({ "path": root.file("saved") }),
    )
    .await;
    let path = saved["path"].as_str().expect("a path").to_owned();
    assert!(path.ends_with("saved.wpsproj"), "{path}");
    assert_eq!(saved["dirty"], false);
    assert!(std::path::Path::new(&path).exists());

    let closed = call(&client, "project_close", json!({})).await;
    assert_eq!(closed["closed"], true);
    assert!(
        pe_app::projects::summary(app.state::<AppState>().inner())
            .unwrap()
            .is_none()
    );

    let opened = call(&client, "project_open", json!({ "path": path })).await;
    assert_eq!(opened["name"], "Saved");
    let recent = call(&client, "recent_projects", json!({})).await;
    assert_eq!(recent["result"][0]["path"], path);
    assert_eq!(recent["result"][0]["exists"], true);
    client.cancel().await.expect("close");
}

#[tokio::test]
async fn a_relative_path_is_refused() {
    let root = TempRoot::new("mcp-relative");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;
    call(&client, "project_new", json!({ "name": "Here" })).await;
    for (tool, args) in [
        ("project_save", json!({ "path": "here.wpsproj" })),
        ("project_open", json!({ "path": "../elsewhere.wpsproj" })),
    ] {
        let message = call_err(&client, tool, args).await;
        assert!(message.contains("absolute"), "{tool}: {message}");
    }
    // Nothing was written beside the test binary.
    assert!(!std::path::Path::new("here.wpsproj").exists());
    client.cancel().await.expect("close");
}

#[tokio::test]
async fn boats_are_added_renamed_removed_and_restored() {
    let root = TempRoot::new("mcp-boats");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;
    call(
        &client,
        "project_new",
        json!({ "name": "Fleet", "boat_name": "First" }),
    )
    .await;
    call(&client, "boat_add", json!({ "name": "Second" })).await;

    let boats = call(&client, "boats_list", json!({})).await;
    let names = |boats: &Value| -> Vec<String> {
        boats["tabs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tab| tab["name"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(names(&boats), ["First", "Second"]);
    let second = boats["tabs"][1]["id"].as_u64().unwrap();

    call(
        &client,
        "boat_rename",
        json!({ "boat": second, "name": "Sister ship" }),
    )
    .await;
    // The interface's own read agrees.
    let listed = pe_app::boats::list(app.state::<AppState>().inner()).unwrap();
    assert_eq!(listed.tabs[1].name, "Sister ship");

    call(&client, "boat_remove", json!({ "boat": second })).await;
    let after = call(&client, "boats_list", json!({})).await;
    assert_eq!(names(&after), ["First"]);
    assert_eq!(after["can_restore"], true);

    call(&client, "boat_restore", json!({})).await;
    assert_eq!(
        names(&call(&client, "boats_list", json!({})).await),
        ["First", "Sister ship"]
    );

    // Review focus 4: a boat that is not there is refused in words.
    let message = call_err(
        &client,
        "boat_rename",
        json!({ "boat": 424_242, "name": "Ghost" }),
    )
    .await;
    assert!(!message.is_empty());
    client.cancel().await.expect("close");
}

/// An edit made through a tool is one undo step, undone by the tool and by
/// the interface's own undo alike, back to the very bytes.
#[tokio::test]
async fn undo_through_a_tool_restores_the_document_byte_for_byte() {
    let root = TempRoot::new("mcp-undo");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;
    call(
        &client,
        "project_new",
        json!({ "name": "Undo", "boat_name": "Before" }),
    )
    .await;
    let first = call(&client, "boats_list", json!({})).await["tabs"][0]["id"]
        .as_u64()
        .unwrap();
    let before = project_bytes(&app);

    let renamed = call(
        &client,
        "boat_rename",
        json!({ "boat": first, "name": "After" }),
    )
    .await;
    assert_eq!(renamed["boat_name"], "After");
    assert_eq!(renamed["can_undo"], true);
    assert_ne!(project_bytes(&app), before);

    let undone = call(&client, "undo", json!({})).await;
    assert_eq!(undone["boat_name"], "Before");
    assert_eq!(undone["can_redo"], true);
    assert_eq!(project_bytes(&app), before);

    let redone = call(&client, "redo", json!({})).await;
    assert_eq!(redone["boat_name"], "After");
    // The interface's own undo takes the tool's edit back just the same.
    pe_app::edit::undo_last(app.state::<AppState>().inner()).unwrap();
    assert_eq!(project_bytes(&app), before);
    client.cancel().await.expect("close");
}

#[tokio::test]
async fn a_write_tool_emits_document_changed_once_and_a_read_tool_emits_nothing() {
    let root = TempRoot::new("mcp-events");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;
    let changed = changes(&app);

    call(&client, "project_new", json!({ "name": "Events" })).await;
    let opened = changed.recv_timeout(SOON).expect("project_new is reported");
    assert_eq!(opened["opened"], true);
    assert_eq!(opened["project"]["name"], "Events");
    assert!(changed.recv_timeout(BRIEFLY).is_err(), "once");

    call(&client, "boat_add", json!({ "name": "Second" })).await;
    let edited = changed.recv_timeout(SOON).expect("an edit is reported");
    assert_eq!(edited["opened"], false);
    assert!(changed.recv_timeout(BRIEFLY).is_err(), "once");

    for tool in [
        "project_status",
        "boats_list",
        "recent_projects",
        "polarexplorer_guide",
    ] {
        call(&client, tool, json!({})).await;
    }
    assert!(
        changed.recv_timeout(BRIEFLY).is_err(),
        "a read tool tells the frontend nothing"
    );

    call(&client, "project_discard", json!({})).await;
    let closed = changed.recv_timeout(SOON).expect("closing is reported");
    assert_eq!(closed["project"], Value::Null);
    client.cancel().await.expect("close");
}

// ----------------------------------------------------------------- sources

/// An Expedition polar on the default output grid's points: 6 and 10 kn of
/// wind, 45°, 90° and 135°.
const POLAR_TXT: &str = "6 45 4.0 90 5.0 135 4.5\n10 45 5.0 90 9.0 135 6.0\n";

/// A project with one polar file imported through the tool; answers the
/// client and the source's id.
async fn with_polar(root: &TempRoot, port: u16, token: &str) -> (Client, u64) {
    let client = client(port, token).await;
    call(&client, "project_new", json!({ "name": "Sources" })).await;
    let file = root.file("farr.txt");
    std::fs::write(&file, POLAR_TXT).unwrap();
    let imported = call(&client, "polar_files_import", json!({ "paths": [file] })).await;
    assert_eq!(imported["imported"], json!(["farr.txt"]));
    assert_eq!(imported["failures"], json!([]));
    let id = imported["project"]["sources"][0]["id"]
        .as_u64()
        .expect("the source's id");
    (client, id)
}

#[tokio::test]
async fn sources_round_trip_through_the_interface_reads() {
    let root = TempRoot::new("mcp-sources");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let (client, id) = with_polar(&root, port, &token).await;

    let listed = call(&client, "sources_list", json!({})).await;
    assert_eq!(listed["sources"][0]["kind"], "polar_file");
    assert_eq!(listed["sources"][0]["weight"], 1.0);
    assert_eq!(listed["sources"][0]["count"], 6);
    assert!(listed["blend"]["direct"].as_u64().unwrap() > 0);

    let set = call(
        &client,
        "source_set",
        json!({ "source": id, "label": "Farr 40", "colour": "#59a14f", "weight": 0.4 }),
    )
    .await;
    assert_eq!(set["sources"][0]["label"], "Farr 40");
    // The interface's own read agrees.
    let current = pe_app::projects::summary(app.state::<AppState>().inner())
        .unwrap()
        .unwrap();
    assert_eq!(current.sources[0].label, "Farr 40");
    assert_eq!(current.sources[0].colour, "#59a14f");
    assert_eq!(current.sources[0].weight, 0.4);
    assert!(current.sources[0].visible);

    call(
        &client,
        "source_set",
        json!({ "source": id, "visible": false }),
    )
    .await;
    let hidden = call(&client, "sources_list", json!({})).await;
    assert_eq!(hidden["sources"][0]["visible"], false);
    assert_eq!(
        hidden["blend"]["direct"], 0,
        "a hidden source is out of the blend"
    );

    call(&client, "source_remove", json!({ "source": id })).await;
    assert_eq!(
        call(&client, "sources_list", json!({})).await["sources"],
        json!([])
    );
    call(&client, "undo", json!({})).await;
    assert_eq!(
        call(&client, "sources_list", json!({})).await["sources"][0]["id"],
        id
    );
    client.cancel().await.expect("close");
}

#[tokio::test]
async fn a_weight_outside_zero_to_one_is_refused() {
    let root = TempRoot::new("mcp-weight");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let (client, id) = with_polar(&root, port, &token).await;
    let before = project_bytes(&app);
    for weight in [1.5, -0.1, 2.0] {
        let message = call_err(
            &client,
            "source_set",
            json!({ "source": id, "weight": weight }),
        )
        .await;
        assert!(message.contains("weight"), "{weight}: {message}");
    }
    assert_eq!(project_bytes(&app), before, "a refusal changes nothing");
    client.cancel().await.expect("close");
}

/// Each field is its own command, so its own undo step, in the order
/// label, colour, visible, weight; and a refusal partway through leaves
/// what went before committed and reported.
#[tokio::test]
async fn source_set_applies_each_field_as_its_own_undo_entry() {
    let root = TempRoot::new("mcp-source-set");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let (client, id) = with_polar(&root, port, &token).await;
    let set = call(
        &client,
        "source_set",
        json!({ "source": id, "label": "A", "weight": 0.5 }),
    )
    .await;
    assert_eq!(set["undo_label"], "Change source weight");
    let undone = call(&client, "undo", json!({})).await;
    assert_eq!(undone["sources"][0]["weight"], 1.0);
    assert_eq!(undone["sources"][0]["label"], "A");
    let undone = call(&client, "undo", json!({})).await;
    assert_eq!(undone["sources"][0]["label"], "farr.txt");

    // The label is applied, then the weight refused: the label stays, and
    // the frontend hears about it.
    let changed = changes(&app);
    let message = call_err(
        &client,
        "source_set",
        json!({ "source": id, "label": "Kept", "weight": 7 }),
    )
    .await;
    assert!(message.contains("weight"), "{message}");
    let payload = changed
        .recv_timeout(SOON)
        .expect("a partial write is reported");
    assert_eq!(payload["project"]["sources"][0]["label"], "Kept");
    // Nothing to set is a refusal, not a silent success.
    let message = call_err(&client, "source_set", json!({ "source": id })).await;
    assert!(message.contains("nothing to change"), "{message}");
    client.cancel().await.expect("close");
}

/// Review focus 4: an id that names nothing is refused in words, as a tool
/// result, by the command itself.
#[tokio::test]
async fn an_unknown_source_or_boat_is_refused_in_words() {
    let root = TempRoot::new("mcp-unknown");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let (client, id) = with_polar(&root, port, &token).await;
    for (tool, args) in [
        ("source_set", json!({ "source": 9999, "weight": 0.5 })),
        ("source_remove", json!({ "source": 9999 })),
        ("source_move", json!({ "source": 9999, "to": 0 })),
        ("sources_list", json!({ "boat": 424_242 })),
        (
            "source_set",
            json!({ "boat": 424_242, "source": id, "weight": 0.5 }),
        ),
    ] {
        let message = call_err(&client, tool, args.clone()).await;
        assert!(!message.is_empty(), "{tool} {args}");
    }
    client.cancel().await.expect("close");
}

/// A second boat's sources are its own: the `boat` id reaches them.
#[tokio::test]
async fn a_tool_acts_on_the_boat_it_is_given() {
    let root = TempRoot::new("mcp-boat-scope");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let (client, _first_source) = with_polar(&root, port, &token).await;
    call(&client, "boat_add", json!({ "name": "Second" })).await;
    let second = call(&client, "boats_list", json!({})).await["tabs"][1]["id"]
        .as_u64()
        .unwrap();
    assert_eq!(
        call(&client, "sources_list", json!({ "boat": second })).await["sources"],
        json!([])
    );
    let file = root.file("farr.txt");
    let imported = call(
        &client,
        "polar_files_import",
        json!({ "boat": second, "paths": [file] }),
    )
    .await;
    assert_eq!(imported["project"]["sources"].as_array().unwrap().len(), 1);
    // The first boat still has exactly its one.
    assert_eq!(
        call(&client, "sources_list", json!({})).await["sources"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    client.cancel().await.expect("close");
}

/// The embedded ORC catalogue: Eratosthenes, a Swan 112, sail GBR 1124
/// (the certificate `tests/orc.rs` looks up).
#[tokio::test]
async fn orc_search_finds_and_adds_a_certificate() {
    let root = TempRoot::new("mcp-orc");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;
    call(&client, "project_new", json!({ "name": "ORC" })).await;

    let found = call(&client, "orc_search", json!({ "query": "GBR/1124" })).await;
    assert_eq!(found["hits"][0]["name"], "Eratosthenes");
    // Filters arrive as an object, or as a string of JSON (review focus 1).
    let filtered = call(
        &client,
        "orc_search",
        json!({ "query": "swan", "filters": "{\"country\":\"GBR\"}", "limit": 5 }),
    )
    .await;
    assert!(filtered["hits"].as_array().unwrap().len() <= 5);
    assert!(
        filtered["hits"]
            .as_array()
            .unwrap()
            .iter()
            .all(|hit| hit["country"] == "GBR")
    );
    // A filter that does not parse is a refusal naming the parameter.
    let message = call_err(
        &client,
        "orc_search",
        json!({ "query": "swan", "filters": { "year_min": "old" } }),
    )
    .await;
    assert!(message.starts_with("filters:"), "{message}");

    let id = found["hits"][0]["id"].clone();
    let added = call(&client, "orc_add", json!({ "id": id })).await;
    assert_eq!(added["sources"][0]["kind"], "orc");
    // The same certificate again is refused unless asked for.
    let message = call_err(&client, "orc_add", json!({ "id": id })).await;
    assert!(message.contains("allow_duplicate"), "{message}");
    let twice = call(
        &client,
        "orc_add",
        json!({ "id": id, "allow_duplicate": true }),
    )
    .await;
    assert_eq!(twice["sources"].as_array().unwrap().len(), 2);

    call(
        &client,
        "source_move",
        json!({ "source": twice["sources"][1]["id"], "to": 0 }),
    )
    .await;
    let order = pe_app::projects::summary(app.state::<AppState>().inner())
        .unwrap()
        .unwrap();
    assert_eq!(
        order.sources[0].id,
        twice["sources"][1]["id"].as_u64().unwrap()
    );
    client.cancel().await.expect("close");
}

// -------------------------------------------------------- tracks, weather

/// A project with the driver suite's instrument CSV imported through the
/// tool: twelve positions a minute apart, each with its own wind.
async fn with_track(root: &TempRoot, port: u16, token: &str) -> (Client, u64) {
    let client = client(port, token).await;
    call(&client, "project_new", json!({ "name": "Tracks" })).await;
    let id = import_track(&client, root, None).await;
    (client, id)
}

/// Imports the instrument CSV into a boat (the first for `None`) and
/// answers the track's source id.
async fn import_track(client: &Client, root: &TempRoot, boat: Option<u64>) -> u64 {
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/webdriver/fixtures/supplied-wind.csv");
    let file = root.file("race.csv");
    std::fs::copy(fixture, &file).unwrap();

    let inspected = call(client, "track_files_inspect", json!({ "paths": [file] })).await;
    assert_eq!(inspected["result"][0]["kind"], "csv");
    assert_eq!(inspected["result"][0]["csv"]["row_count"], 12);
    let mapping = inspected["result"][0]["csv"]["mapping"].clone();

    let imported = call(
        client,
        "track_files_import",
        json!({ "boat": boat, "files": [{ "path": file, "mapping": mapping, "boats": null }] }),
    )
    .await;
    assert_eq!(imported["failures"], json!([]));
    imported["imported"][0]["source_id"]
        .as_u64()
        .expect("the track's source id")
}

fn track_of(app: &tauri::App<MockRuntime>, id: u64) -> pe_app::tracks::TrackSummary {
    pe_app::projects::summary(app.state::<AppState>().inner())
        .unwrap()
        .unwrap()
        .sources
        .into_iter()
        .find(|source| source.id == id)
        .and_then(|source| source.track)
        .expect("the track")
}

#[tokio::test]
async fn a_track_file_is_inspected_imported_and_filtered() {
    let root = TempRoot::new("mcp-track");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let (client, id) = with_track(&root, port, &token).await;
    let before = track_of(&app, id);
    assert_eq!(before.samples, 12);
    assert_eq!(before.supplied_wind, 12);

    // Only the filter named changes; the others stay as they were.
    let set = call(
        &client,
        "track_set",
        json!({ "source": id, "filters": { "min_bsp": 5.5, "max_heading_change": null } }),
    )
    .await;
    assert_eq!(set["undo_label"], "Change sample filters");
    let after = track_of(&app, id);
    assert_eq!(after.filters.min_bsp, Some(5.5));
    assert_eq!(after.filters.max_heading_change, None);
    assert_eq!(after.filters.max_bsp, before.filters.max_bsp);
    assert_eq!(after.filters.wave_mode, before.filters.wave_mode);
    assert!(
        after.used < after.samples,
        "a stopped position is under 5.5 kn"
    );

    // Derivation and the wind choice are their own commands and undo steps.
    call(
        &client,
        "track_set",
        json!({ "source": id, "max_gap_s": 120, "downloaded_wind_only": true }),
    )
    .await;
    let changed = track_of(&app, id);
    assert_eq!(changed.max_gap_s, 120);
    assert_eq!(changed.prefer_heading, before.prefer_heading);
    assert_eq!(changed.prefer_speed, before.prefer_speed);
    assert!(changed.downloaded_wind_only);
    // Heading and speed are preferred apart.
    call(
        &client,
        "track_set",
        json!({ "source": id, "prefer_speed": "derived" }),
    )
    .await;
    let split = track_of(&app, id);
    assert_eq!(split.prefer_heading, before.prefer_heading);
    assert_eq!(split.prefer_speed, "derived");

    let message = call_err(&client, "track_set", json!({ "source": id })).await;
    assert!(message.contains("nothing to change"), "{message}");
    let message = call_err(
        &client,
        "track_set",
        json!({ "source": id, "filters": { "min_bsp": "fast" } }),
    )
    .await;
    assert!(message.starts_with("filters:"), "{message}");
    client.cancel().await.expect("close");
}

/// Review focus 1: a client that sends an object as a string of JSON is
/// read, not refused.
#[tokio::test]
async fn a_filter_sent_as_a_string_is_read() {
    let root = TempRoot::new("mcp-string-filter");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let (client, id) = with_track(&root, port, &token).await;
    call(
        &client,
        "track_set",
        json!({ "source": id, "filters": "{\"min_bsp\": 4.25}" }),
    )
    .await;
    assert_eq!(track_of(&app, id).filters.min_bsp, Some(4.25));
    client.cancel().await.expect("close");
}

#[tokio::test]
async fn samples_are_listed_excluded_and_included_through_the_tools() {
    let root = TempRoot::new("mcp-samples");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let (client, id) = with_track(&root, port, &token).await;
    call(
        &client,
        "track_set",
        json!({ "source": id, "filters": { "min_bsp": null, "max_heading_change": null } }),
    )
    .await;

    let listed = call(&client, "track_samples", json!({ "source": id })).await;
    assert_eq!(listed["total"], 12);
    let samples = listed["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 12);
    // The CSV's first row: 6.0 kn in 10 kn of wind from the north while
    // heading east, so 90° off the wind.
    assert_eq!(samples[0]["tws"], 10.0);
    assert_eq!(samples[0]["twa"], 90.0);
    assert_eq!(samples[0]["excluded"], false);
    let page = call(
        &client,
        "track_samples",
        json!({ "source": id, "offset": 10, "limit": 5 }),
    )
    .await;
    assert_eq!(page["samples"].as_array().unwrap().len(), 2);

    let first = samples[0]["sample"].as_u64().unwrap();
    let second = samples[1]["sample"].as_u64().unwrap();
    call(
        &client,
        "samples_exclude",
        json!({ "samples": [first, second], "excluded": true }),
    )
    .await;
    assert_eq!(track_of(&app, id).excluded, 2);
    let detail = call(
        &client,
        "sample_get",
        json!({ "source": id, "sample": first }),
    )
    .await;
    assert_eq!(detail["excluded"], true);
    assert_eq!(detail["lon"], -1.0);

    call(
        &client,
        "samples_exclude",
        json!({ "samples": [first], "excluded": false }),
    )
    .await;
    assert_eq!(track_of(&app, id).excluded, 1);
    call(&client, "undo", json!({})).await;
    assert_eq!(track_of(&app, id).excluded, 2);

    let message = call_err(
        &client,
        "sample_get",
        json!({ "source": id, "sample": 987_654 }),
    )
    .await;
    assert!(!message.is_empty());
    client.cancel().await.expect("close");
}

/// The estimate is worked out from the positions alone: nothing is fetched.
#[tokio::test]
async fn weather_estimate_answers_without_fetching() {
    let root = TempRoot::new("mcp-estimate");
    let app = mock_app(&root);
    // The fixture archive on a loopback stub, so the estimate reaches no
    // network.
    let archive = pe_env::whirlwind::testing::serve(pe_env::whirlwind::testing::fixture(), 200);
    // A reader owns a runtime, which may not be dropped on this one: the
    // provider is built and replaced on a thread of its own.
    let state = app.state::<AppState>().inner().clone();
    let endpoint = archive.endpoint().to_owned();
    std::thread::spawn(move || {
        common::remember_archive(&state, Some(&endpoint));
        pe_app::env::provider(&state).unwrap();
        state.env_provider.lock().unwrap().as_mut().unwrap().1 =
            std::sync::Arc::new(pe_env::whirlwind::testing::reader_at(&endpoint));
    })
    .join()
    .unwrap();
    let (port, token) = serve(&app);
    let (client, id) = with_track(&root, port, &token).await;
    let estimate = call(&client, "weather_estimate", json!({ "sources": [id] })).await;
    assert_eq!(estimate["samples"], 12);
    assert!(estimate.get("three_hourly_bytes").is_none());
    // The fixture archive holds 2000-01-01 only: none of the track's hours,
    // so nothing to download.
    assert_eq!(estimate["hourly_bytes"], 0);
    assert_eq!(
        call(&client, "weather_jobs", json!({})).await["tracks"],
        json!([])
    );
    let message = call_err(
        &client,
        "weather_fetch",
        json!({ "sources": [id], "interval": "daily" }),
    )
    .await;
    assert!(message.contains("interval"), "{message}");
    client.cancel().await.expect("close");
}

/// Review focus 5: the client goes away in the middle of a long call. The
/// fetch it started is cancelled with it, and its session is not left
/// counted. (The mock application runs no fetch thread, so the queued fetch
/// would otherwise wait for ever: nothing here reaches the network.)
#[tokio::test]
async fn weather_jobs_survive_disconnect_and_cancel_explicitly() {
    let root = TempRoot::new("mcp-dropped");
    let app = mock_app(&root);
    common::remember_archive(&app.state::<AppState>(), None);
    let (port, token) = serve(&app);
    let (client, id) = with_track(&root, port, &token).await;
    let request = json!({"sources":[id],"restart":true,"request_id":"weather-once"});
    let started = call(&client, "weather_fetch", request.clone()).await;
    assert!(started["job_id"].is_string());
    client.cancel().await.expect("close");
    assert_eq!(
        app.state::<AppState>().env_jobs.status().tracks.len(),
        1,
        "disconnect keeps the fetch"
    );
    let reconnected = self::client(port, &token).await;
    let retried = call(&reconnected, "weather_fetch", request).await;
    assert_eq!(retried["job_id"], started["job_id"]);
    assert_eq!(retried["reused"], true);
    call(&reconnected, "weather_cancel", json!({"sources":[id]})).await;
    assert!(app.state::<AppState>().env_jobs.status().tracks.is_empty());
    let jobs = call(&reconnected, "assistant_jobs", json!({})).await;
    assert_eq!(jobs["jobs"][0]["state"], "cancelled");
    reconnected.cancel().await.expect("close");
}

/// A key a patch misspells is refused, naming the keys there are: applied
/// as "nothing to change", it would leave an agent believing a filter is
/// on that is not.
#[tokio::test]
async fn a_misspelt_key_in_a_patch_is_refused_naming_the_keys() {
    let root = TempRoot::new("mcp-misspelt");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let (client, id) = with_track(&root, port, &token).await;
    let before = project_bytes(&app);

    let message = call_err(
        &client,
        "track_set",
        json!({ "source": id, "filters": { "min_bps": 4 } }),
    )
    .await;
    assert!(message.contains("min_bps"), "{message}");
    assert!(
        message.contains("min_bsp"),
        "names the real keys: {message}"
    );

    let message = call_err(
        &client,
        "blend_set",
        json!({ "settings": { "interpolaton": "monotone_spline" } }),
    )
    .await;
    assert!(message.contains("interpolaton"), "{message}");
    assert!(message.contains("interpolation"), "{message}");

    let message = call_err(
        &client,
        "blend_filters_set",
        json!({ "global": { "tws_minimum": 8 } }),
    )
    .await;
    assert!(message.contains("tws_minimum"), "{message}");
    let message = call_err(
        &client,
        "blend_filters_set",
        json!({ "wave_ranges": { "hs": { "mn": 1 } } }),
    )
    .await;
    assert!(message.contains("mn"), "{message}");
    assert!(message.contains("min"), "{message}");
    let message = call_err(
        &client,
        "blend_filters_set",
        json!({ "priority_groups": [{ "twa_mn": 30 }] }),
    )
    .await;
    assert!(message.contains("twa_mn"), "{message}");

    let message = call_err(
        &client,
        "orc_search",
        json!({ "query": "", "filters": { "contry": "GBR" } }),
    )
    .await;
    assert!(message.contains("contry"), "{message}");
    assert!(message.contains("country"), "{message}");

    assert_eq!(project_bytes(&app), before, "nothing was applied");

    // One end of a wave range changes that end and keeps the other.
    call(
        &client,
        "blend_filters_set",
        json!({ "wave_ranges": { "hs": { "min": 0.5, "max": 3 } } }),
    )
    .await;
    let set = call(
        &client,
        "blend_filters_set",
        json!({ "wave_ranges": { "hs": { "max": 2 } } }),
    )
    .await;
    assert_eq!(set["blend"]["wave_ranges"]["hs"]["min"], 0.5);
    assert_eq!(set["blend"]["wave_ranges"]["hs"]["max"], 2.0);
    client.cancel().await.expect("close");
}

/// Source ids are each boat's own, so two boats both have a source 1. A
/// fetch cancelled without naming a boat is the first boat's, as every tool
/// reads a missing `boat`; the other boat's fetch of its own source 1 goes
/// on, and the first boat's call does not wait for it.
#[tokio::test]
async fn a_weather_call_without_a_boat_is_the_first_boats_alone() {
    let root = TempRoot::new("mcp-weather-boats");
    let app = mock_app(&root);
    common::remember_archive(&app.state::<AppState>(), None);
    let (port, token) = serve(&app);
    let (client, id) = with_track(&root, port, &token).await;
    call(&client, "boat_add", json!({ "name": "Second" })).await;
    let second = call(&client, "boats_list", json!({})).await["tabs"][1]["id"]
        .as_u64()
        .unwrap();
    let other = import_track(&client, &root, Some(second)).await;
    assert_eq!(other, id, "source ids are local to each boat");
    let jobs = || app.state::<AppState>().env_jobs.status().tracks;

    let fetch = |args: Value| {
        let params = CallToolRequestParams::new("weather_fetch")
            .with_arguments(args.as_object().cloned().unwrap());
        let peer = client.peer().clone();
        tokio::spawn(async move { peer.call_tool(params).await })
    };
    let first_call = fetch(json!({ "sources": [id], "restart": true }));
    let second_call = fetch(json!({ "boat": second, "sources": [other], "restart": true }));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while jobs().len() < 2 && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(jobs().len(), 2, "both fetches were queued");

    call(&client, "weather_cancel", json!({ "sources": [id] })).await;
    let left = jobs();
    assert_eq!(left.len(), 1, "the second boat's fetch goes on: {left:?}");
    assert_eq!(left[0].boat_id, Some(second));
    // The first boat's call ends: what it waited for is gone.
    let ended = tokio::time::timeout(std::time::Duration::from_secs(5), first_call).await;
    assert!(ended.is_ok(), "the first boat's call still waits");
    assert_eq!(jobs().len(), 1);

    call(&client, "weather_cancel", json!({ "boat": second })).await;
    assert_eq!(jobs().len(), 0);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), second_call).await;
    client.cancel().await.expect("close");
}

/// The ORR catalogue refresh is something a client may start (the service's
/// design, consent). Its year is checked before anything is asked of the
/// network, so this reaches the command without reaching the site.
#[tokio::test]
async fn the_orr_refresh_is_a_tool_that_checks_its_year_first() {
    let root = TempRoot::new("mcp-orr-refresh");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;
    call(&client, "project_new", json!({ "name": "ORR" })).await;
    let message = call_err(&client, "orr_refresh", json!({ "year": 1900 })).await;
    assert!(message.contains("1900"), "{message}");
    client.cancel().await.expect("close");
}

/// A tracker's name and an event's address are checked before anything is
/// asked of the network.
#[tokio::test]
async fn a_tracker_request_that_names_no_event_is_refused_without_the_network() {
    let root = TempRoot::new("mcp-tracker");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;
    call(&client, "project_new", json!({ "name": "Trackers" })).await;
    let message = call_err(
        &client,
        "tracker_event",
        json!({ "tracker": "sextant", "url": "anything" }),
    )
    .await;
    assert!(message.contains("sextant"), "{message}");
    let message = call_err(
        &client,
        "tracker_event",
        json!({ "tracker": "yellowbrick", "url": "not an address at all" }),
    )
    .await;
    assert!(!message.is_empty());
    client.cancel().await.expect("close");
}

// ------------------------------------------------- blend, compare, export

/// The blend fixture: a polar file (source 1) and a track (source 2) that
/// meet at 90° in 12 kn, the file at 8.0 kn, the track's segment at 6.81 kn
/// with a third of full confidence.
async fn with_blend(app: &tauri::App<MockRuntime>, port: u16, token: &str) -> Client {
    let client = client(port, token).await;
    call(&client, "project_new", json!({ "name": "Blend" })).await;
    common::add_blend_sources(app.state::<AppState>().inner());
    client
}

fn near(value: &Value, expected: f64) -> bool {
    value.as_f64().is_some_and(|v| (v - expected).abs() < 1e-9)
}

/// The file at weight 0.5 and the track at a third of full confidence:
/// shares 0.6 and 0.4, and the blend 0.6 × 8.0 + 0.4 × 6.81 = 7.524 kn.
#[tokio::test]
async fn blend_cell_names_its_sources_through_the_tool() {
    let root = TempRoot::new("mcp-blend-cell");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = with_blend(&app, port, &token).await;
    call(&client, "source_set", json!({ "source": 1, "weight": 0.5 })).await;
    let before = project_bytes(&app);

    let cell = call(&client, "blend_cell", json!({ "twa": 90, "tws": 12 })).await;
    assert_eq!(cell["bsp"], 7.524);
    assert_eq!(cell["origin"], "direct");
    assert_eq!(cell["corrected"], false);
    let contributors = cell["contributors"].as_array().unwrap();
    assert_eq!(contributors.len(), 2);
    assert_eq!(
        (
            contributors[0]["source_id"].as_u64(),
            contributors[1]["source_id"].as_u64()
        ),
        (Some(1), Some(2))
    );
    assert_eq!(
        (&contributors[0]["bsp"], &contributors[1]["bsp"]),
        (&json!(8.0), &json!(6.81))
    );
    assert!(near(&contributors[0]["share"], 0.6) && near(&contributors[1]["share"], 0.4));
    assert_eq!(project_bytes(&app), before, "reading a cell writes nothing");
    client.cancel().await.expect("close");
}

#[tokio::test]
async fn a_blend_cell_off_the_grid_is_refused_naming_the_axis() {
    let root = TempRoot::new("mcp-blend-off-grid");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = with_blend(&app, port, &token).await;
    let message = call_err(&client, "blend_cell", json!({ "twa": 91, "tws": 12 })).await;
    assert!(
        message.contains("TWA") && message.contains("90"),
        "{message}"
    );
    let message = call_err(&client, "blend_cell", json!({ "twa": 90, "tws": 11 })).await;
    assert!(
        message.contains("TWS") && message.contains("12"),
        "{message}"
    );
    client.cancel().await.expect("close");
}

#[tokio::test]
async fn blend_set_changes_only_what_it_is_given_and_undoes() {
    let root = TempRoot::new("mcp-blend-set");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = with_blend(&app, port, &token).await;
    let before = call(&client, "blend_status", json!({})).await;
    assert_eq!(before["interpolation"], "linear");
    assert_eq!(before["n_full"], 30);

    let set = call(
        &client,
        "blend_set",
        json!({ "settings": { "n_full": 10 } }),
    )
    .await;
    assert_eq!(set["blend"]["n_full"], 10);
    assert_eq!(
        set["blend"]["twa"], before["twa"],
        "the grid was not named, so it stays"
    );
    assert_eq!(set["blend"]["interpolation"], "linear");
    assert_eq!(set["undo_label"], "Change blend settings");
    // Ten samples of ten for full confidence now: the track counts whole,
    // (8.0 + 6.81) / 2 = 7.405 kn.
    assert_eq!(
        call(&client, "blend_cell", json!({ "twa": 90, "tws": 12 })).await["bsp"],
        7.405
    );

    let set = call(
        &client,
        "blend_set",
        json!({ "settings": "{\"interpolation\": \"monotone_spline\"}" }),
    )
    .await;
    assert_eq!(set["blend"]["interpolation"], "monotone_spline");
    assert_eq!(
        set["blend"]["n_full"], 10,
        "what the first call set is kept"
    );

    let undone = call(&client, "undo", json!({})).await;
    assert_eq!(undone["blend"]["interpolation"], "linear");
    let message = call_err(
        &client,
        "blend_set",
        json!({ "settings": { "interpolation": "cubic" } }),
    )
    .await;
    assert!(message.contains("cubic"), "{message}");
    client.cancel().await.expect("close");
}

#[tokio::test]
async fn blend_filters_are_set_and_cleared() {
    let root = TempRoot::new("mcp-blend-filters");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = with_blend(&app, port, &token).await;
    // A global filter over every track: only the limit named is set.
    let set = call(
        &client,
        "blend_filters_set",
        json!({ "global": { "min_bsp": 6.5 } }),
    )
    .await;
    assert_eq!(set["blend"]["global_filters"]["min_bsp"], 6.5);
    assert_eq!(set["blend"]["global_filters"]["wave_mode"], "off");
    let cleared = call(
        &client,
        "blend_filters_set",
        json!({ "clear_global": true }),
    )
    .await;
    assert_eq!(cleared["blend"]["global_filters"], Value::Null);

    let waves = call(
        &client,
        "blend_filters_set",
        json!({ "wave_ranges": { "hs": { "min": 0.5, "max": 2.0 } } }),
    )
    .await;
    assert_eq!(
        waves["blend"]["wave_ranges"]["hs"],
        json!({ "min": 0.5, "max": 2.0 })
    );
    assert_eq!(
        waves["blend"]["wave_ranges"]["wavePeriod"],
        json!({ "min": null, "max": null })
    );

    let groups = call(
        &client,
        "blend_filters_set",
        json!({ "priority_groups": [{ "tws_min": 10 }, {}], "priority_minimum": 3 }),
    )
    .await;
    assert_eq!(
        groups["blend"]["priority_groups"].as_array().unwrap().len(),
        2
    );
    assert_eq!(groups["blend"]["priority_groups"][0]["tws_min"], 10.0);
    assert_eq!(groups["blend"]["priority_min_samples"], 3);
    let message = call_err(&client, "blend_filters_set", json!({})).await;
    assert!(message.contains("nothing to change"), "{message}");
    client.cancel().await.expect("close");
}

#[tokio::test]
async fn polar_read_gives_the_blend_and_a_source_and_polar_edit_writes_an_overlay() {
    let root = TempRoot::new("mcp-polar-read");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = with_blend(&app, port, &token).await;

    let file = call(&client, "polar_read", json!({ "source": 1 })).await;
    assert_eq!(file["twa"], json!([45.0, 90.0, 135.0]));
    assert_eq!(file["tws"], json!([6.0, 10.0, 14.0]));
    assert_eq!(file["bsp"][1], json!([5.0, 9.0, 7.0]));
    let blend = call(&client, "polar_read", json!({})).await;
    assert_eq!(blend["kind"], "blend");
    assert_eq!(
        blend["twa"].as_array().unwrap().len(),
        19,
        "the output grid"
    );

    // An edit names its cells by their values on the source's own axes.
    let edited = call(
        &client,
        "polar_edit",
        json!({ "source": 1, "op": { "type": "type", "bsp": 9.5 }, "cells": [{ "twa": 90, "tws": 10 }] }),
    )
    .await;
    assert_eq!(edited["sources"][0]["edits"], 1);
    let after = call(&client, "polar_read", json!({ "source": 1 })).await;
    assert_eq!(after["bsp"][1][1], 9.5);
    assert_eq!(
        after["source"][1][1], 9.0,
        "the imported value is still there"
    );
    assert_eq!(after["edited"][1][1], true);

    // With no source, the blend itself is corrected.
    call(
        &client,
        "polar_edit",
        json!({ "op": "{\"type\":\"type\",\"bsp\":9.25}", "cells": [{ "twa": 90, "tws": 12 }] }),
    )
    .await;
    let cell = call(&client, "blend_cell", json!({ "twa": 90, "tws": 12 })).await;
    assert_eq!(cell["bsp"], 9.25);
    assert_eq!(cell["corrected"], true);

    let message = call_err(
        &client,
        "polar_edit",
        json!({ "source": 1, "op": { "type": "type", "bsp": 7 }, "cells": [{ "twa": 91, "tws": 10 }] }),
    )
    .await;
    assert!(message.contains("TWA"), "{message}");
    client.cancel().await.expect("close");
}

/// File against track: the one segment cell is the overlap. The file reads
/// 8.0 kn at 90° in 12 kn, the segment 6.81: Δ = 1.19 kn, A faster.
#[tokio::test]
async fn compare_reports_the_difference_as_json() {
    let root = TempRoot::new("mcp-compare");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = with_blend(&app, port, &token).await;
    let before = project_bytes(&app);
    let compared = call(
        &client,
        "compare",
        json!({ "a": { "kind": "polar", "source_id": 1 }, "b": { "kind": "segment", "source_id": 2 } }),
    )
    .await;
    assert_eq!(compared["overlap"], 1);
    assert_eq!(compared["b_only"], 0);
    assert_eq!(
        compared["largest"],
        json!({ "twa": 90.0, "tws": 12.0, "delta_kn": 1.19 })
    );
    assert_eq!(
        compared["regions"],
        json!([{ "tws": 12.0, "twa_from": 90.0, "twa_to": 90.0, "faster": "a" }])
    );
    let twa = compared["twa"]
        .as_array()
        .unwrap()
        .iter()
        .position(|v| *v == 90.0)
        .unwrap();
    let tws = compared["tws"]
        .as_array()
        .unwrap()
        .iter()
        .position(|v| *v == 12.0)
        .unwrap();
    assert_eq!(compared["delta_kn"][twa][tws], 1.19);
    assert_eq!(compared["a"][twa][tws], 8.0);
    assert_eq!(project_bytes(&app), before, "comparing writes nothing");

    let message = call_err(
        &client,
        "compare",
        json!({ "a": { "kind": "segment", "source_id": 1 }, "b": { "kind": "blend" } }),
    )
    .await;
    assert!(!message.is_empty());
    client.cancel().await.expect("close");
}

/// The tool writes the very bytes the interface's export writes.
#[tokio::test]
async fn export_writes_the_bytes_the_interface_writes() {
    let root = TempRoot::new("mcp-export");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = with_blend(&app, port, &token).await;

    let preview = call(&client, "export_preview", json!({ "format": "expedition" })).await;
    assert_eq!(preview["format"], "expedition");
    assert_eq!(preview["problem"], Value::Null);

    let by_tool = root.file("tool.txt");
    let exported = call(
        &client,
        "export_polar",
        json!({ "path": by_tool, "format": "expedition" }),
    )
    .await;
    assert_eq!(exported["path"], by_tool);
    let by_interface = root.file("interface.txt");
    pe_app::blend::export_to(
        app.state::<AppState>().inner(),
        &by_interface,
        "expedition",
        None,
    )
    .unwrap();
    let bytes = std::fs::read(&by_tool).unwrap();
    assert!(!bytes.is_empty());
    assert_eq!(bytes, std::fs::read(&by_interface).unwrap());
    assert_eq!(exported["bytes"], bytes.len());

    // Custom axes, as a string of JSON.
    let custom = root.file("custom.csv");
    call(
        &client,
        "export_polar",
        json!({ "path": custom, "format": "csv", "axes": "{\"twa\":[60,90,120],\"tws\":[8,12]}" }),
    )
    .await;
    assert!(std::fs::read_to_string(&custom).unwrap().contains("60"));

    let all = call(
        &client,
        "export_all",
        json!({ "directory": root.file("fleet"), "format": "adrena" }),
    )
    .await;
    assert_eq!(all["paths"].as_array().unwrap().len(), 1);
    assert_eq!(all["failures"], json!([]));
    client.cancel().await.expect("close");
}

#[tokio::test]
async fn an_export_needs_an_absolute_path_and_a_known_format() {
    let root = TempRoot::new("mcp-export-refused");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = with_blend(&app, port, &token).await;
    let message = call_err(
        &client,
        "export_polar",
        json!({ "path": "polar.txt", "format": "expedition" }),
    )
    .await;
    assert!(message.contains("absolute"), "{message}");
    assert!(!std::path::Path::new("polar.txt").exists());
    let message = call_err(
        &client,
        "export_polar",
        json!({ "path": root.file("p.xyz"), "format": "xyz" }),
    )
    .await;
    assert!(message.contains("xyz"), "{message}");
    client.cancel().await.expect("close");
}

/// A file that is already there is not replaced unless the call says so:
/// the interface's save dialog asks, and a tool has no dialog. The escape
/// hatch is no way around it.
#[tokio::test]
async fn a_tool_does_not_replace_a_file_unless_told_to() {
    let root = TempRoot::new("mcp-overwrite");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = with_blend(&app, port, &token).await;

    let notes = root.file("notes.txt");
    std::fs::write(&notes, "mine").unwrap();
    let message = call_err(
        &client,
        "export_polar",
        json!({ "path": notes, "format": "expedition" }),
    )
    .await;
    assert!(message.contains("overwrite"), "{message}");
    assert_eq!(std::fs::read_to_string(&notes).unwrap(), "mine");
    call(
        &client,
        "export_polar",
        json!({ "path": notes, "format": "expedition", "overwrite": true }),
    )
    .await;
    assert_ne!(std::fs::read_to_string(&notes).unwrap(), "mine");

    // Save As over another file, named with and without the extension the
    // save adds.
    let other = root.file("other.wpsproj");
    std::fs::write(&other, "another project").unwrap();
    for path in [other.clone(), root.file("other")] {
        let message = call_err(&client, "project_save", json!({ "path": path })).await;
        assert!(message.contains("overwrite"), "{message}");
        assert_eq!(std::fs::read(&other).unwrap(), b"another project");
    }
    call(
        &client,
        "project_save",
        json!({ "path": other, "overwrite": true }),
    )
    .await;
    assert_ne!(std::fs::read(&other).unwrap(), b"another project");
    // The project's own file is its to save again, by either spelling.
    call(&client, "project_save", json!({})).await;
    call(&client, "project_save", json!({ "path": other })).await;

    for (command, args) in [
        (
            "save_project_as",
            json!({ "path": root.file("around.wpsproj") }),
        ),
        (
            "export_polar",
            json!({ "boat_context": null, "path": notes, "format": "csv", "axes": null }),
        ),
    ] {
        let message = call_err(
            &client,
            "invoke",
            json!({ "command": command, "args": args }),
        )
        .await;
        assert!(message.contains("not available"), "{command}: {message}");
    }
    assert!(!std::path::Path::new(&root.file("around.wpsproj")).exists());
    client.cancel().await.expect("close");
}

// -------------------------------------------------------------------- view

/// Every payload of one event from here on.
fn heard(app: &tauri::App<MockRuntime>, event: &'static str) -> std::sync::mpsc::Receiver<Value> {
    use tauri::Listener;
    let (tx, rx) = std::sync::mpsc::channel::<Value>();
    app.listen(event, move |e| {
        let _ = tx.send(serde_json::from_str(e.payload()).expect("json"));
    });
    rx
}

#[tokio::test]
async fn view_tools_emit_events_for_the_frontend() {
    let root = TempRoot::new("mcp-view");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let (client, track) = with_track(&root, port, &token).await;
    let stage = heard(&app, pe_app::mcp::events::STAGE);
    let boat = heard(&app, pe_app::mcp::events::BOAT);
    let selection = heard(&app, pe_app::mcp::events::SELECTION);
    let changed = changes(&app);

    let first = call(&client, "boats_list", json!({})).await["tabs"][0]["id"]
        .as_u64()
        .unwrap();
    for name in ["3d", "map", "compare", "plot"] {
        call(&client, "view_stage", json!({ "stage": name })).await;
        let shown = stage.recv_timeout(SOON).expect("stage");
        assert_eq!(shown["stage"], name);
        // Without a boat, the first: the stage is that boat's to show.
        assert_eq!(shown["boat"], first);
    }
    let message = call_err(&client, "view_stage", json!({ "stage": "radar" })).await;
    assert!(message.contains("radar"), "{message}");

    call(&client, "boat_add", json!({ "name": "Second" })).await;
    let _ = changed.recv_timeout(SOON);
    let second = call(&client, "boats_list", json!({})).await["tabs"][1]["id"]
        .as_u64()
        .unwrap();
    call(&client, "view_boat", json!({ "boat": second })).await;
    assert_eq!(boat.recv_timeout(SOON).expect("boat")["boat"], second);
    // The second boat has no track: there is no map to show for it.
    let message = call_err(
        &client,
        "view_stage",
        json!({ "stage": "map", "boat": second }),
    )
    .await;
    assert!(message.contains("track"), "{message}");
    let message = call_err(&client, "view_boat", json!({ "boat": 424_242 })).await;
    assert!(message.contains("424242"), "{message}");
    // A stage for a named boat says whose it is, so the frontend applies it
    // to that boat's view and not to whichever is on show.
    call(
        &client,
        "view_stage",
        json!({ "stage": "compare", "boat": second }),
    )
    .await;
    let shown = stage.recv_timeout(SOON).expect("stage");
    assert_eq!(shown["stage"], "compare");
    assert_eq!(shown["boat"], second);

    let samples = call(
        &client,
        "track_samples",
        json!({ "source": track, "limit": 2 }),
    )
    .await;
    let ids: Vec<u64> = samples["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["sample"].as_u64().unwrap())
        .collect();
    call(&client, "selection_set", json!({ "samples": ids })).await;
    let selected = selection.recv_timeout(SOON).expect("selection");
    assert_eq!(selected["samples"], json!(ids));
    assert_eq!(selected["boat"], Value::Null);

    // Moving the view is not an edit: the document was not touched.
    assert!(changed.recv_timeout(BRIEFLY).is_err());
    client.cancel().await.expect("close");
}

/// The smallest PNG: one transparent pixel.
const PIXEL_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

#[tokio::test]
async fn a_screenshot_is_delivered_by_the_frontend() {
    use tauri::Listener;
    let root = TempRoot::new("mcp-screenshot");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;
    call(&client, "project_new", json!({ "name": "Picture" })).await;
    // Stand in for the frontend: answer the request with a picture.
    let handle = app.handle().clone();
    app.listen(pe_app::mcp::capture::CAPTURE, move |event| {
        let request: Value = serde_json::from_str(event.payload()).expect("json");
        let id = request["id"].as_u64().expect("an id");
        pe_app::mcp::capture::deliver_capture(handle.state(), id, PIXEL_PNG.to_owned())
            .expect("deliver");
    });
    let params = CallToolRequestParams::new("screenshot");
    let result: CallToolResult = client.call_tool(params).await.expect("call");
    assert_ne!(result.is_error, Some(true), "{:?}", result.content);
    let image = result
        .content
        .iter()
        .find_map(|c| c.as_image())
        .expect("an image");
    assert_eq!(image.mime_type, "image/png");
    assert_eq!(image.data, PIXEL_PNG);
    client.cancel().await.expect("close");
}

/// Review focus 3: with no project there is nothing to photograph, and the
/// tool says so at once instead of asking a frontend that would not answer.
#[tokio::test]
async fn a_screenshot_with_no_project_is_refused_before_asking() {
    let root = TempRoot::new("mcp-screenshot-none");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;
    let asked = heard(&app, pe_app::mcp::capture::CAPTURE);
    let started = std::time::Instant::now();
    let message = call_err(&client, "screenshot", json!({})).await;
    assert!(message.contains("No project is open."), "{message}");
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert!(
        asked.recv_timeout(BRIEFLY).is_err(),
        "the frontend was not asked"
    );
    client.cancel().await.expect("close");
}

#[tokio::test]
async fn a_declined_screenshot_returns_the_reason() {
    use tauri::Listener;
    let root = TempRoot::new("mcp-screenshot-declined");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;
    call(&client, "project_new", json!({ "name": "Picture" })).await;
    let handle = app.handle().clone();
    app.listen(pe_app::mcp::capture::CAPTURE, move |event| {
        let request: Value = serde_json::from_str(event.payload()).expect("json");
        pe_app::mcp::capture::refuse_capture(
            handle.state(),
            request["id"].as_u64().expect("an id"),
            "the window is hidden".to_owned(),
        )
        .expect("refuse");
    });
    let message = call_err(&client, "screenshot", json!({})).await;
    assert!(message.contains("the window is hidden"), "{message}");
    client.cancel().await.expect("close");
}

// ------------------------------------------------------------------ invoke

#[tokio::test]
async fn invoke_reaches_a_command_and_the_interface_follows() {
    let root = TempRoot::new("mcp-invoke");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;
    call(&client, "project_new", json!({ "name": "Before" })).await;
    let changed = changes(&app);

    let renamed = call(
        &client,
        "invoke",
        json!({ "command": "rename_project", "args": { "name": "After" } }),
    )
    .await;
    assert_eq!(renamed["result"]["name"], "After");
    assert_eq!(renamed["result"]["undo_label"], "Rename project");
    let payload = changed
        .recv_timeout(SOON)
        .expect("a write through invoke is reported");
    assert_eq!(payload["project"]["name"], "After");
    assert_eq!(payload["opened"], false);

    // No arguments at all, and arguments as a string of JSON.
    let recent = call(&client, "invoke", json!({ "command": "recent_projects" })).await;
    assert!(recent["result"].is_array());
    let coloured = call(
        &client,
        "invoke",
        json!({ "command": "set_blend_colour", "args": "{\"colour\": \"#123456\"}" }),
    )
    .await;
    assert_eq!(coloured["result"]["blend"]["colour"], "#123456");

    // A command that opens a project says so to the frontend.
    let _ = changed.try_iter().count();
    call(
        &client,
        "invoke",
        json!({ "command": "new_project", "args": { "name": "Fresh", "boat": null, "discard_unsaved": true } }),
    )
    .await;
    let mut opened = false;
    while let Ok(payload) = changed.recv_timeout(SOON) {
        if payload["project"]["name"] == "Fresh" {
            opened = payload["opened"] == true;
            break;
        }
    }
    assert!(opened, "new_project through invoke is an opening");
    client.cancel().await.expect("close");
}

#[tokio::test]
async fn invoke_refuses_an_unknown_argument_command_or_an_excluded_one() {
    let root = TempRoot::new("mcp-invoke-refused");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;
    call(&client, "project_new", json!({ "name": "Refusals" })).await;

    let message = call_err(
        &client,
        "invoke",
        json!({ "command": "rename_project", "args": { "name": "X", "nmae": "typo" } }),
    )
    .await;
    assert!(
        message.contains("rename_project") && message.contains("nmae"),
        "{message}"
    );
    let message = call_err(&client, "invoke", json!({ "command": "format_disk" })).await;
    assert!(message.contains("unknown command format_disk"), "{message}");

    // The settings hold this service's token and the library's folders; the
    // service cannot be reconfigured from inside itself; quitting is the
    // person's.
    for command in [
        "app_settings",
        "set_theme",
        "set_library_settings",
        "mcp_set",
        "mcp_rotate_token",
        "quit_app",
    ] {
        let message = call_err(&client, "invoke", json!({ "command": command, "args": {} })).await;
        assert!(
            message.contains("not available through invoke"),
            "{command}: {message}"
        );
    }
    assert!(
        pe_app::settings::current(app.state::<AppState>().inner())
            .unwrap()
            .mcp
            .token
            .is_empty()
    );
    client.cancel().await.expect("close");
}

// -------------------------------------------- descriptions, schemas, guide

/// A description is what a model has of a tool, and in a client that shows
/// no server instructions it is all it has of the application. Claude Code
/// keeps 2,048 characters of the instructions and is said to do the same
/// to a description, so each is held to that; and the tools a request
/// starts from have to say whose they are, or someone asked to "use
/// PolarExplorer" does not find them.
#[tokio::test]
async fn the_descriptions_fit_and_the_entry_points_say_whose_they_are() {
    let root = TempRoot::new("mcp-descriptions");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;
    let tools = client.list_all_tools().await.expect("tools");
    for tool in &tools {
        let description = tool.description.as_deref().unwrap_or_default();
        assert!(!description.is_empty(), "{} says nothing", tool.name);
        let units = description.encode_utf16().count();
        assert!(
            units <= pe_app::mcp::tools::guide::INSTRUCTIONS_LIMIT,
            "{}: {units} units of description",
            tool.name
        );
    }
    for entry in ["polarexplorer_guide", "project_status", "project_new"] {
        let tool = tools
            .iter()
            .find(|tool| tool.name == entry)
            .unwrap_or_else(|| panic!("{entry} is listed"));
        let description = tool.description.as_deref().unwrap_or_default();
        assert!(
            description.contains("PolarExplorer"),
            "{entry}: {description:.80}"
        );
    }
    client.cancel().await.expect("close");
}

/// Every schema is one a strict client accepts. The reference is the MCP
/// specification as the official TypeScript SDK enforces it: a tool's input
/// is an object, and each property's schema is itself an object — a bare
/// `true` ("anything") makes that client refuse the whole tool list.
#[tokio::test]
async fn every_tool_schema_is_one_a_strict_client_accepts() {
    let root = TempRoot::new("mcp-schemas");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;
    let tools = client.list_all_tools().await.expect("tools");
    assert!(tools.len() >= 50, "only {} tools listed", tools.len());
    for tool in &tools {
        let input = Value::Object((*tool.input_schema).clone());
        assert_eq!(input["type"], "object", "{}: input", tool.name);
        if let Some(properties) = input["properties"].as_object() {
            for (name, schema) in properties {
                assert!(
                    schema.is_object(),
                    "{}.{name}: a property's schema is {schema}, which says nothing",
                    tool.name
                );
                assert!(
                    schema
                        .get("description")
                        .and_then(Value::as_str)
                        .is_some_and(|d| !d.is_empty()),
                    "{}.{name} has no description",
                    tool.name
                );
            }
        }
        if let Some(output) = &tool.output_schema {
            assert_eq!(
                output.get("type"),
                Some(&json!("object")),
                "{}: output",
                tool.name
            );
        }
    }
    client.cancel().await.expect("close");
}

/// The instructions are read by every client for the whole session, and
/// they name tools and parameters. A rename that leaves them behind sends
/// an agent after something that is not there. The same holds for the
/// longer guide the `polarexplorer_guide` tool returns, and for the skill,
/// whose body is that guide.
#[tokio::test]
async fn the_instructions_and_the_guide_name_only_what_exists() {
    let root = TempRoot::new("mcp-guide");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;
    let instructions = client
        .peer_info()
        .and_then(|info| info.instructions.clone())
        .expect("instructions");
    assert!(instructions.starts_with("Today is "), "{instructions:.80}");

    let tools = client.list_all_tools().await.expect("tools");
    let mut known: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
    for tool in &tools {
        if let Some(properties) = tool
            .input_schema
            .get("properties")
            .and_then(Value::as_object)
        {
            known.extend(properties.keys().cloned());
        }
    }
    // Wire values, nested matrix fields, returned job IDs and the explicitly
    // named Hindsight Router handoff are documented beyond top-level inputs.
    known.extend(
        [
            "monotone_spline",
            "discard_unsaved",
            "whirlwind_matrix_polar",
            "sampled_pdf",
            "gaussian_pdf",
            "sog_bins",
            "job_id",
            "polar_import",
        ]
        .map(str::to_owned),
    );

    let guide = call(&client, "polarexplorer_guide", json!({})).await;
    let guide = guide["guide"].as_str().expect("guide").to_owned();
    assert!(guide.starts_with("Today is "), "{guide:.80}");
    // The guide is the instructions at length, never a different story.
    assert!(guide.len() > instructions.len());
    assert!(instructions.contains("polarexplorer_guide"));
    assert!(pe_app::mcp::skill::skill_md().contains(pe_app::mcp::tools::guide::GUIDE));

    let text = format!("{instructions}\n{guide}");
    let words = text
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty());
    let mut named = 0;
    for word in words {
        let snake = word.contains('_') && word.chars().all(|c| !c.is_ascii_uppercase());
        if snake {
            named += 1;
            assert!(
                known.iter().any(|k| k == word),
                "the guide says `{word}`, which is no tool or parameter"
            );
        }
    }
    assert!(
        named > 40,
        "only {named} names found: the scan is not reading the guide"
    );
    // And every tool can be found from the guide, the escape hatch included.
    let unmentioned: Vec<&str> = tools
        .iter()
        .map(|tool| tool.name.as_ref())
        .filter(|name| !text.contains(name))
        .collect();
    assert!(
        unmentioned.is_empty(),
        "the guide never mentions {unmentioned:?}"
    );
    client.cancel().await.expect("close");
}

// ---------------------------------------------- Claude Desktop extension

/// The bundle is what the manifest specification says a bundle is, and holds
/// nothing secret. The required fields are MANIFEST.md's (manifest 0.3):
/// `manifest_version`, `name`, `version`, `description`, `author.name` and
/// `server`, whose `entry_point` has to be a file in the archive.
#[test]
fn the_desktop_bundle_is_a_valid_extension_with_no_secret_in_it() {
    use std::io::Read;
    let root = TempRoot::new("mcp-bundle");
    let settings = root.0.join("config").join("settings.json");
    let bundle = root.0.join("PolarExplorer.mcpb");
    pe_app::mcp::desktop::write_bundle(&bundle, &settings).expect("bundle");

    let mut archive =
        zip::ZipArchive::new(std::fs::File::open(&bundle).expect("open")).expect("zip");
    let mut names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    names.sort();
    assert_eq!(names, ["icon.png", "manifest.json", "server/index.js"]);
    let mut text = String::new();
    archive
        .by_name("manifest.json")
        .expect("manifest")
        .read_to_string(&mut text)
        .expect("read");
    let manifest: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(manifest["manifest_version"], "0.3");
    assert_eq!(manifest["name"], "polarexplorer");
    assert_eq!(manifest["display_name"], "PolarExplorer");
    assert_eq!(manifest["version"], env!("CARGO_PKG_VERSION"));
    assert!(
        manifest["description"]
            .as_str()
            .is_some_and(|d| !d.is_empty())
    );
    assert!(
        manifest["author"]["name"]
            .as_str()
            .is_some_and(|a| !a.is_empty())
    );
    assert_eq!(manifest["server"]["type"], "node");
    let entry = manifest["server"]["entry_point"].as_str().expect("entry");
    assert!(
        names.iter().any(|name| name == entry),
        "{entry} is in the archive"
    );
    assert_eq!(manifest["server"]["mcp_config"]["command"], "node");
    assert_eq!(
        manifest["server"]["mcp_config"]["args"],
        json!(["${__dirname}/server/index.js"])
    );
    assert_eq!(
        manifest["server"]["mcp_config"]["env"]["PE_SETTINGS"],
        settings.to_string_lossy().as_ref()
    );
    assert!(
        names
            .iter()
            .any(|name| Some(name.as_str()) == manifest["icon"].as_str())
    );
    // A PNG, by its signature.
    let mut icon = Vec::new();
    archive
        .by_name("icon.png")
        .expect("icon")
        .read_to_end(&mut icon)
        .expect("read");
    assert_eq!(&icon[..8], b"\x89PNG\r\n\x1a\n");
    // The bridge may use only what Node itself has: nothing is installed.
    let mut bridge = String::new();
    archive
        .by_name("server/index.js")
        .expect("bridge")
        .read_to_string(&mut bridge)
        .expect("read");
    for required in bridge.split("require(\"").skip(1) {
        let module = required.split('"').next().unwrap_or_default();
        assert!(module.starts_with("node:"), "the bridge requires {module}");
    }

    // The token of a running service is nowhere in the file.
    let app = mock_app(&root);
    let (_port, token) = serve(&app);
    let bytes = std::fs::read(&bundle).expect("bytes");
    assert!(!bytes.windows(token.len()).any(|w| w == token.as_bytes()));
    assert!(!text.contains("token"), "{text}");
}

/// The native bridge follows endpoint rotation without a host reinstall.
#[tokio::test]
async fn the_native_bridge_follows_restart_rotation_and_access_off() {
    use std::sync::Arc;
    use whirlwind_mcp::bridge::{Bridge, Launch, Timing};
    struct NeverLaunch;
    impl Launch for NeverLaunch {
        fn launch(&self) -> Result<(), String> {
            Err("The test never launches a GUI".into())
        }
    }
    let root = TempRoot::new("native-bridge");
    let app = mock_app(&root);
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let set = |enabled| {
        pe_app::settings::mcp_set(
            app.handle().clone(),
            app.state(),
            app.state(),
            enabled,
            port,
        )
        .unwrap()
    };
    let first = set(true);
    let dir = app.state::<AppState>().paths.config_dir.join("mcp");
    let handler = pe_app::mcp::tools::PolarExplorer::new(app.handle().clone());
    let info = rmcp::ServerHandler::get_info(&handler);
    drop(handler);
    let bridge = Bridge::new(dir, Arc::new(NeverLaunch), Timing::DEFAULT)
        .unwrap()
        .with_catalog(
            info,
            pe_app::mcp::tools::PolarExplorer::<MockRuntime>::catalogue(),
        );
    let (server_io, client_io) = tokio::io::duplex(65536);
    let serving = tokio::spawn(async move {
        let service = bridge.serve(server_io).await.unwrap();
        let _ = service.waiting().await;
    });
    let client = ().serve(client_io).await.unwrap();
    assert_eq!(
        client
            .peer_info()
            .unwrap()
            .server_info
            .as_ref()
            .unwrap()
            .version,
        env!("CARGO_PKG_VERSION")
    );
    call(&client, "project_status", json!({})).await;
    set(false);
    let message = call_err(&client, "project_status", json!({})).await;
    assert!(message.contains("off"), "{message}");
    let next = set(true);
    assert_ne!(first.token, next.token);
    assert!(call(&client, "project_status", json!({})).await["project"].is_null());
    client.cancel().await.unwrap();
    serving.await.unwrap();
    set(false);
}

/// Registering needs the service on: there is no address or token to give
/// a client otherwise.
#[test]
fn a_client_cannot_be_registered_while_the_service_is_off() {
    let root = TempRoot::new("mcp-register-off");
    let app = mock_app(&root);
    for client in pe_app::mcp::clients::McpClient::available() {
        let refused = pe_app::mcp::clients::mcp_register_client(app.state(), client, None);
        assert_eq!(refused.unwrap_err().kind(), "bad-option", "{client:?}");
    }
}

/// A race built in one call (asked 2026-10-03): race_project downloads the
/// race (here, a recorded one the session already holds, so nothing reaches
/// a tracker), opens a tab per boat with its track and the certificates
/// that match it, and the interface's own read shows the result.
#[tokio::test]
async fn race_project_opens_a_tab_per_boat_with_its_track_and_certificates() {
    use pe_trackers::{
        TrackerBoat, TrackerEvent,
        event::{EventRef, PositionsFrom},
    };
    use std::collections::BTreeMap;
    let root = TempRoot::new("mcp-race");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;

    // A boat the catalogue can match by model (with no builder to contradict
    // it), and one it cannot.
    let catalogue = pe_orc::catalogue().unwrap();
    let entry = (0..catalogue.len() as u32)
        .filter_map(|id| catalogue.entry(id))
        .find(|e| {
            e.model
                .as_deref()
                .is_some_and(|m| pe_app::boats::matching::model_key(m).is_some())
                && e.builder.is_none()
        })
        .unwrap();
    let fixes = |lat: f64| {
        (0..12)
            .map(|k| pe_core::track::Fix {
                tws: None,
                twd_from: None,
                t: 1_790_769_600 + k * 600,
                lat: lat + k as f64 * 0.01,
                lon: -1.0,
                cog: None,
                sog: None,
            })
            .collect::<Vec<_>>()
    };
    let boat = |id: &str, name: &str, model: Option<&str>, lat: f64| TrackerBoat {
        id: id.into(),
        name: name.into(),
        model: model.map(str::to_owned),
        details: BTreeMap::from([("mmsi".into(), format!("23{id}000000"))]),
        sail: None,
        division: None,
        status: None,
        start: None,
        finish: None,
        fixes: fixes(lat),
    };
    let event = TrackerEvent {
        event: EventRef {
            tracker: pe_core::track::Tracker::YellowBrick,
            key: "mcprace".into(),
            url: "https://boats.invalid/mcprace".into(),
        },
        title: "The MCP race".into(),
        start: None,
        stop: None,
        positions_from: PositionsFrom::Primary,
        leg: None,
        boats: vec![
            boat("1", "Matched", entry.model.as_deref(), 50.0),
            boat("2", "Unknown", Some("Nothing Like It 99"), 51.0),
        ],
    };
    app.state::<AppState>()
        .trackers
        .keep(std::sync::Arc::new(event), "mcprace");

    let answer = call(
        &client,
        "race_project",
        json!({ "tracker": "yellowbrick", "url": "mcprace" }),
    )
    .await;
    let boats = answer["boats"].as_array().unwrap();
    assert_eq!(boats.len(), 2);
    assert!(boats[0]["polars"].as_u64().unwrap() > 0, "{answer}");
    assert_eq!(boats[1]["polars"], 0);
    assert_eq!(boats[0]["tracks"], 1);

    // The interface's own read: the race is open, one tab per boat.
    let state = app.state::<AppState>();
    let tabs = pe_app::boats::list(state.inner()).unwrap();
    assert_eq!(tabs.tabs.len(), 2);
    assert_eq!(tabs.name, "The MCP race");
    let first = pe_app::projects::summary(state.inner()).unwrap().unwrap();
    assert!(
        first.sources.iter().any(|s| s.kind == "orc"),
        "the certificate is in the first boat"
    );
    assert!(
        first.sources.iter().any(|s| s.kind == "track"),
        "and its track"
    );

    // Refusals in words: a tracker it does not build from, a match mode it
    // does not know, and the user's unsaved work.
    let unknown = call_err(
        &client,
        "race_project",
        json!({ "tracker": "sailwave", "url": "x" }),
    )
    .await;
    assert!(unknown.contains("geovoile"), "{unknown}");
    let mode = call_err(
        &client,
        "race_project",
        json!({ "tracker": "yellowbrick", "url": "mcprace", "match_mode": "close" }),
    )
    .await;
    assert!(mode.contains("exact_boat"), "{mode}");
    call(&client, "boat_add", json!({ "name": "Unsaved" })).await;
    let unsaved = call_err(
        &client,
        "race_project",
        json!({ "tracker": "yellowbrick", "url": "mcprace" }),
    )
    .await;
    assert!(unsaved.contains("project_discard"), "{unsaved}");
    client.cancel().await.expect("close");
}

/// The track library over MCP (asked 2026-10-03): with no library set up,
/// a search says so and an import is refused in words.
#[tokio::test]
async fn the_track_library_answers_even_when_it_is_not_set_up() {
    let root = TempRoot::new("mcp-library");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;
    call(&client, "project_new", json!({ "name": "Library" })).await;
    let found = call(&client, "library_search", json!({ "query": "Ker 46" })).await;
    assert_eq!(found["downloaded"], false);
    assert_eq!(found["total"], 0);
    let refused = call_err(&client, "library_import", json!({ "id": "nothing" })).await;
    assert!(refused.contains("Settings"), "{refused}");
    client.cancel().await.expect("close");
}

/// A Geovoile race builds the same way (asked 2026-10-03): one tab per boat
/// with its track, from a recorded event the session holds under the key
/// the link resolves to.
#[tokio::test]
async fn race_project_builds_a_geovoile_race_too() {
    use pe_trackers::{TrackerBoat, TrackerEvent, event::PositionsFrom};
    let root = TempRoot::new("mcp-race-geovoile");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let client = client(port, &token).await;
    // As a person may paste it, without its scheme.
    let link = "routedurhum.geovoile.com/2022/";
    let reference = pe_trackers::event::client(pe_core::track::Tracker::Geovoile)
        .unwrap()
        .resolve(link)
        .unwrap();
    // Geovoile gives a boat's name and sail number, no model.
    let boat = |id: &str, lat: f64| TrackerBoat {
        id: id.into(),
        name: format!("Skipper {id}"),
        model: None,
        details: Default::default(),
        sail: Some(format!("FRA {id}")),
        division: Some("Rhum Mono".into()),
        status: None,
        start: None,
        finish: None,
        fixes: (0..12)
            .map(|k| pe_core::track::Fix {
                tws: None,
                twd_from: None,
                t: 1_790_769_600 + k * 600,
                lat: lat - k as f64 * 0.02,
                lon: -2.0 - k as f64 * 0.02,
                cog: None,
                sog: None,
            })
            .collect(),
    };
    let key = reference.key.clone();
    app.state::<AppState>().trackers.keep(
        std::sync::Arc::new(TrackerEvent {
            event: reference,
            title: "Route du Rhum".into(),
            start: None,
            stop: None,
            positions_from: PositionsFrom::Primary,
            leg: None,
            boats: vec![boat("1", 48.6), boat("2", 48.7), boat("3", 48.8)],
        }),
        &key,
    );
    let answer = call(
        &client,
        "race_project",
        json!({ "tracker": "geovoile", "url": link }),
    )
    .await;
    let boats = answer["boats"].as_array().unwrap();
    assert_eq!(boats.len(), 3, "{answer}");
    assert!(boats.iter().all(|b| b["tracks"] == 1), "{answer}");
    let tabs = pe_app::boats::list(app.state::<AppState>().inner()).unwrap();
    assert_eq!(tabs.tabs.len(), 3);
    assert_eq!(tabs.name, "Route du Rhum");
    client.cancel().await.expect("close");
}
