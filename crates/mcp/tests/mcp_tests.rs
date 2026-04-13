use std::fs;

use codebones_mcp::CodebonesMcpServer;
use rmcp::{
    model::{CallToolRequestParams, ErrorCode},
    service::ServiceError,
    ServiceExt,
};
use tempfile::TempDir;

#[tokio::test]
async fn mcp_server_exposes_real_tools_over_transport() -> anyhow::Result<()> {
    let dir = TempDir::new().expect("temp repo");
    fs::write(
        dir.path().join("lib.rs"),
        "pub fn compat() -> &'static str { \"ok\" }\n",
    )
    .expect("write fixture");

    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server = CodebonesMcpServer::new();
    let server_handle = tokio::spawn(async move {
        let running = server.serve(server_transport).await?;
        running.waiting().await?;
        anyhow::Ok(())
    });

    let client = ().serve(client_transport).await?;

    let tools = client.peer().list_tools(None).await?;
    let tool_names: Vec<_> = tools.tools.iter().map(|tool| tool.name.as_ref()).collect();
    assert!(tool_names.contains(&"index"));
    assert!(tool_names.contains(&"outline"));
    assert!(tool_names.contains(&"get"));
    assert!(tool_names.contains(&"search"));
    assert!(
        tool_names.contains(&"map"),
        "map tool must be registered; got: {:?}",
        tool_names
    );
    assert!(
        tool_names.contains(&"graph"),
        "graph tool must be registered; got: {:?}",
        tool_names
    );
    assert!(
        tool_names.contains(&"graph_file"),
        "graph_file tool must be registered; got: {:?}",
        tool_names
    );

    let index_result = client
        .call_tool(
            CallToolRequestParams::new("index").with_arguments(
                rmcp::serde_json::json!({
                    "dir": dir.path().to_string_lossy(),
                })
                .as_object()
                .expect("index arguments must be an object")
                .clone(),
            ),
        )
        .await?;

    let index_text = index_result
        .content
        .first()
        .and_then(|content| content.raw.as_text())
        .map(|text| text.text.as_str())
        .expect("index response should include text content");
    assert!(index_text.contains("indexed"));

    let search_result = client
        .call_tool(
            CallToolRequestParams::new("search").with_arguments(
                rmcp::serde_json::json!({
                    "dir": dir.path().to_string_lossy(),
                    "query": "compat",
                })
                .as_object()
                .expect("search arguments must be an object")
                .clone(),
            ),
        )
        .await?;

    let search_text = search_result
        .content
        .first()
        .and_then(|content| content.raw.as_text())
        .map(|text| text.text.as_str())
        .expect("search response should include text content");
    assert!(search_text.contains("lib.rs::compat"));

    let get_result = client
        .call_tool(
            CallToolRequestParams::new("get").with_arguments(
                rmcp::serde_json::json!({
                    "dir": dir.path().to_string_lossy(),
                    "symbolOrPath": "lib.rs::compat",
                })
                .as_object()
                .expect("get arguments must be an object")
                .clone(),
            ),
        )
        .await?;

    let get_text = get_result
        .content
        .first()
        .and_then(|content| content.raw.as_text())
        .map(|text| text.text.as_str())
        .expect("get response should include text content");
    assert!(get_text.contains("compat"));

    let outline_result = client
        .call_tool(
            CallToolRequestParams::new("outline").with_arguments(
                rmcp::serde_json::json!({
                    "dir": dir.path().to_string_lossy(),
                    "path": "lib.rs",
                })
                .as_object()
                .expect("outline arguments must be an object")
                .clone(),
            ),
        )
        .await?;

    let outline_text = outline_result
        .content
        .first()
        .and_then(|content| content.raw.as_text())
        .map(|text| text.text.as_str())
        .expect("outline response should include text content");
    assert!(outline_text.contains("pub fn compat()"));

    // --- map tool: returns skeleton without file contents ---
    let map_result = client
        .call_tool(
            CallToolRequestParams::new("map").with_arguments(
                rmcp::serde_json::json!({
                    "dir": dir.path().to_string_lossy(),
                })
                .as_object()
                .expect("map arguments must be an object")
                .clone(),
            ),
        )
        .await?;

    let map_text = map_result
        .content
        .first()
        .and_then(|content| content.raw.as_text())
        .map(|text| text.text.as_str())
        .expect("map response should include text content");
    assert!(
        map_text.contains("lib.rs"),
        "map output must contain the indexed file; got: {}",
        map_text
    );
    assert!(
        map_text.contains("compat"),
        "map output must contain the symbol name; got: {}",
        map_text
    );

    // --- graph tool: returns import graph ---
    let graph_result = client
        .call_tool(
            CallToolRequestParams::new("graph").with_arguments(
                rmcp::serde_json::json!({
                    "dir": dir.path().to_string_lossy(),
                })
                .as_object()
                .expect("graph arguments must be an object")
                .clone(),
            ),
        )
        .await?;

    let graph_text = graph_result
        .content
        .first()
        .and_then(|content| content.raw.as_text())
        .map(|text| text.text.as_str())
        .expect("graph response should include text content");
    assert!(
        graph_text.contains("lib.rs"),
        "graph output must contain indexed files; got: {}",
        graph_text
    );

    // --- graph_file tool: returns blast radius ---
    let graph_file_result = client
        .call_tool(
            CallToolRequestParams::new("graph_file").with_arguments(
                rmcp::serde_json::json!({
                    "dir": dir.path().to_string_lossy(),
                    "file": "lib.rs",
                })
                .as_object()
                .expect("graph_file arguments must be an object")
                .clone(),
            ),
        )
        .await?;

    let graph_file_text = graph_file_result
        .content
        .first()
        .and_then(|content| content.raw.as_text())
        .map(|text| text.text.as_str())
        .expect("graph_file response should include text content");
    assert!(
        graph_file_text.contains("Blast Radius"),
        "graph_file output must contain blast radius header; got: {}",
        graph_file_text
    );

    client.cancel().await?;
    server_handle.await??;
    Ok(())
}

// ---------------------------------------------------------------------------
// MCP protective defaults — RED tests (must fail until implementation lands)
// ---------------------------------------------------------------------------

/// AC1: MCP `graph` called WITHOUT `top` defaults to top=50.
///
/// We create 55 distinct files, index the repo, call `graph` over MCP without
/// passing a `top` argument, and assert that the output contains at most 50
/// file entries.  Currently the server applies NO default, so all 55 files
/// will appear — this test fails until the default is wired in.
#[tokio::test]
async fn mcp_graph_without_top_defaults_to_50() -> anyhow::Result<()> {
    let dir = TempDir::new().expect("temp repo");
    // Create 55 files so the fixture exceeds the expected default of 50.
    for i in 0..55_usize {
        fs::write(
            dir.path().join(format!("file_{i:03}.rs")),
            format!("pub fn func_{i}() {{}}\n"),
        )
        .expect("write fixture file");
    }

    let (server_transport, client_transport) = tokio::io::duplex(65536);
    let server = CodebonesMcpServer::new();
    let server_handle = tokio::spawn(async move {
        let running = server.serve(server_transport).await?;
        running.waiting().await?;
        anyhow::Ok(())
    });

    let client = ().serve(client_transport).await?;

    // Index first.
    client
        .call_tool(
            CallToolRequestParams::new("index").with_arguments(
                rmcp::serde_json::json!({ "dir": dir.path().to_string_lossy() })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await?;

    // Call `graph` WITHOUT a `top` parameter — MCP default should cap at 50.
    let result = client
        .call_tool(
            CallToolRequestParams::new("graph").with_arguments(
                rmcp::serde_json::json!({ "dir": dir.path().to_string_lossy() })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await?;

    let text = result
        .content
        .first()
        .and_then(|c| c.raw.as_text())
        .map(|t| t.text.as_str())
        .expect("graph response must contain text");

    // Count how many "file_" entries appear in the markdown output.
    // Each file appears as a line like: `- `file_NNN.rs` — imported by ...`
    let file_entry_count = text.matches("file_").count();
    assert!(
        file_entry_count <= 50,
        "MCP graph without top= must return at most 50 files (got {} entries):\n{}",
        file_entry_count,
        &text[..text.len().min(500)]
    );

    client.cancel().await?;
    server_handle.await??;
    Ok(())
}

/// AC2: MCP `graph` called WITH an explicit `top` parameter respects that value.
///
/// We create 10 files, call `graph` with `top=5`, and assert no more than 5
/// file entries appear in the output.  This tests the existing explicit-top
/// code path; it should pass even before the default is added.
#[tokio::test]
async fn mcp_graph_with_explicit_top_respects_value() -> anyhow::Result<()> {
    let dir = TempDir::new().expect("temp repo");
    for i in 0..10_usize {
        fs::write(
            dir.path().join(format!("mod_{i}.rs")),
            format!("pub fn fn_{i}() {{}}\n"),
        )
        .expect("write fixture file");
    }

    let (server_transport, client_transport) = tokio::io::duplex(32768);
    let server = CodebonesMcpServer::new();
    let server_handle = tokio::spawn(async move {
        let running = server.serve(server_transport).await?;
        running.waiting().await?;
        anyhow::Ok(())
    });

    let client = ().serve(client_transport).await?;

    client
        .call_tool(
            CallToolRequestParams::new("index").with_arguments(
                rmcp::serde_json::json!({ "dir": dir.path().to_string_lossy() })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await?;

    let result = client
        .call_tool(
            CallToolRequestParams::new("graph").with_arguments(
                rmcp::serde_json::json!({
                    "dir": dir.path().to_string_lossy(),
                    "top": 5
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await?;

    let text = result
        .content
        .first()
        .and_then(|c| c.raw.as_text())
        .map(|t| t.text.as_str())
        .expect("graph response must contain text");

    let file_entry_count = text.matches("mod_").count();
    assert!(
        file_entry_count <= 5,
        "MCP graph with top=5 must return at most 5 files (got {} entries):\n{}",
        file_entry_count,
        &text[..text.len().min(500)]
    );

    client.cancel().await?;
    server_handle.await??;
    Ok(())
}

/// AC5: MCP `map` called WITHOUT `maxTokens` defaults to maxTokens=50000.
///
/// We need to observe that the default cap is applied.  We construct a fixture
/// large enough that the full skeleton would exceed 50 000 tokens, then verify
/// the output is shorter than it would be without any cap.
///
/// Because generating a truly huge fixture is slow, we use a moderate fixture
/// and assert the output size against a known-unlimited call with a very large
/// explicit cap (1 000 000).  Until the server applies the default, the two
/// calls will return identical output, causing the assertion to fail.
#[tokio::test]
async fn mcp_map_without_max_tokens_defaults_to_50000() -> anyhow::Result<()> {
    let dir = TempDir::new().expect("temp repo");
    // Write many files whose skeleton collectively exceeds 50 000 tokens.
    // Each function signature costs ~10 tokens; 6 000 functions ≈ 60 000 tokens.
    // Split across 20 files to stay under the 500 KB per-file indexer limit.
    for file_idx in 0..20_usize {
        let mut content = String::new();
        for i in 0..300_usize {
            let global_i = file_idx * 300 + i;
            content.push_str(&format!(
                "pub fn very_long_function_name_to_pad_tokens_{global_i:05}(arg_one: u64, arg_two: u64) -> u64 {{ arg_one + arg_two }}\n"
            ));
        }
        fs::write(dir.path().join(format!("part_{file_idx:02}.rs")), &content)
            .expect("write part fixture");
    }

    let (server_transport, client_transport) = tokio::io::duplex(1 << 20);
    let server = CodebonesMcpServer::new();
    let server_handle = tokio::spawn(async move {
        let running = server.serve(server_transport).await?;
        running.waiting().await?;
        anyhow::Ok(())
    });

    let client = ().serve(client_transport).await?;

    client
        .call_tool(
            CallToolRequestParams::new("index").with_arguments(
                rmcp::serde_json::json!({ "dir": dir.path().to_string_lossy() })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await?;

    // Call map WITHOUT maxTokens — should apply the 50 000 default.
    let default_result = client
        .call_tool(
            CallToolRequestParams::new("map").with_arguments(
                rmcp::serde_json::json!({ "dir": dir.path().to_string_lossy() })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await?;

    // Call map WITH a very large explicit cap to get the "unlimited" baseline.
    let unlimited_result = client
        .call_tool(
            CallToolRequestParams::new("map").with_arguments(
                rmcp::serde_json::json!({
                    "dir": dir.path().to_string_lossy(),
                    "maxTokens": 1_000_000_usize
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await?;

    let default_text = default_result
        .content
        .first()
        .and_then(|c| c.raw.as_text())
        .map(|t| t.text.as_str())
        .expect("map default response must contain text");

    let unlimited_text = unlimited_result
        .content
        .first()
        .and_then(|c| c.raw.as_text())
        .map(|t| t.text.as_str())
        .expect("map unlimited response must contain text");

    assert!(
        default_text.len() < unlimited_text.len(),
        "MCP map without maxTokens must be shorter than map with maxTokens=1_000_000 \
         (default={} bytes, unlimited={} bytes); \
         the 50 000-token default is not being applied",
        default_text.len(),
        unlimited_text.len()
    );

    client.cancel().await?;
    server_handle.await??;
    Ok(())
}

#[tokio::test]
async fn mcp_server_returns_specific_errors_for_invalid_dir_and_missing_paths() -> anyhow::Result<()>
{
    let dir = TempDir::new().expect("temp repo");
    fs::write(
        dir.path().join("lib.rs"),
        "pub fn compat() -> &'static str { \"ok\" }\n",
    )
    .expect("write fixture");

    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server = CodebonesMcpServer::new();
    let server_handle = tokio::spawn(async move {
        let running = server.serve(server_transport).await?;
        running.waiting().await?;
        anyhow::Ok(())
    });

    let client = ().serve(client_transport).await?;

    let invalid_dir_error = client
        .call_tool(
            CallToolRequestParams::new("index").with_arguments(
                rmcp::serde_json::json!({
                    "dir": dir.path().join("missing").to_string_lossy(),
                })
                .as_object()
                .expect("index arguments must be an object")
                .clone(),
            ),
        )
        .await
        .expect_err("invalid dir should return an MCP error");
    match invalid_dir_error {
        ServiceError::McpError(error) => {
            assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
            assert!(error.message.contains("index failed"));
        }
        other => panic!("unexpected error type: {other:?}"),
    }

    client
        .call_tool(
            CallToolRequestParams::new("index").with_arguments(
                rmcp::serde_json::json!({
                    "dir": dir.path().to_string_lossy(),
                })
                .as_object()
                .expect("index arguments must be an object")
                .clone(),
            ),
        )
        .await?;

    let missing_symbol_error = client
        .call_tool(
            CallToolRequestParams::new("get").with_arguments(
                rmcp::serde_json::json!({
                    "dir": dir.path().to_string_lossy(),
                    "symbolOrPath": "missing.rs::compat",
                })
                .as_object()
                .expect("get arguments must be an object")
                .clone(),
            ),
        )
        .await
        .expect_err("missing symbol should return an MCP error");
    match missing_symbol_error {
        ServiceError::McpError(error) => {
            assert_eq!(error.code, ErrorCode::RESOURCE_NOT_FOUND);
            assert!(error.message.contains("get failed"));
        }
        other => panic!("unexpected error type: {other:?}"),
    }

    let missing_path_error = client
        .call_tool(
            CallToolRequestParams::new("outline").with_arguments(
                rmcp::serde_json::json!({
                    "dir": dir.path().to_string_lossy(),
                    "path": "missing.rs",
                })
                .as_object()
                .expect("outline arguments must be an object")
                .clone(),
            ),
        )
        .await
        .expect_err("missing path should return an MCP error");
    match missing_path_error {
        ServiceError::McpError(error) => {
            assert_eq!(error.code, ErrorCode::RESOURCE_NOT_FOUND);
            assert!(error.message.contains("outline failed"));
        }
        other => panic!("unexpected error type: {other:?}"),
    }

    client.cancel().await?;
    server_handle.await??;
    Ok(())
}
