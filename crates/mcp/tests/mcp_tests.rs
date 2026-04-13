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
