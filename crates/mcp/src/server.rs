use std::path::Path;

use rmcp::schemars::JsonSchema;
use rmcp::serde::{Deserialize, Serialize};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    tool, tool_handler, tool_router, ErrorData, Json, ServerHandler,
};

fn format_graph_mcp(
    result: &codebones_core::api::GraphResult,
    format: &str,
    top: Option<usize>,
) -> String {
    match format {
        "json" => {
            let files_json: Vec<String> = result
                .files
                .iter()
                .map(|f| {
                    format!(
                        r#"{{"path":"{}","import_count":{}}}"#,
                        f.path.replace('"', "\\\""),
                        f.import_count
                    )
                })
                .collect();
            let edges_json: Vec<String> = result
                .edges
                .iter()
                .map(|e| {
                    format!(
                        r#"{{"from":"{}","to":"{}"}}"#,
                        e.from.replace('"', "\\\""),
                        e.to.replace('"', "\\\"")
                    )
                })
                .collect();
            format!(
                r#"{{"files":[{}],"edges":[{}]}}"#,
                files_json.join(","),
                edges_json.join(",")
            )
        }
        "xml" => {
            let mut out = String::from("<graph>\n<files>\n");
            for f in &result.files {
                out.push_str(&format!(
                    "  <file path=\"{}\" import_count=\"{}\"/>\n",
                    f.path.replace('&', "&amp;").replace('"', "&quot;"),
                    f.import_count
                ));
            }
            out.push_str("</files>\n<edges>\n");
            for e in &result.edges {
                out.push_str(&format!(
                    "  <edge from=\"{}\" to=\"{}\"/>\n",
                    e.from.replace('&', "&amp;").replace('"', "&quot;"),
                    e.to.replace('&', "&amp;").replace('"', "&quot;")
                ));
            }
            out.push_str("</edges>\n</graph>");
            out
        }
        _ => {
            let mut out = String::from("# Import Graph\n\n## Most Imported Files\n");
            for f in &result.files {
                out.push_str(&format!(
                    "- `{}` — imported by **{}** files\n",
                    f.path, f.import_count
                ));
            }
            if top.is_none() {
                out.push_str("\n## Import Map\n");
                for e in &result.edges {
                    out.push_str(&format!("- `{}` → {}\n", e.from, e.to));
                }
            }
            out
        }
    }
}

fn format_blast_radius_mcp(file_path: &str, affected: &[String]) -> String {
    let mut out = format!(
        "# Blast Radius: {}\n\n## Affected Files ({})\n",
        file_path,
        affected.len()
    );
    for f in affected {
        out.push_str(&format!("- {}\n", f));
    }
    out
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct IndexArgs {
    dir: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct IndexResponse {
    dir: String,
    status: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct OutlineArgs {
    dir: String,
    path: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct OutlineResponse {
    dir: String,
    path: String,
    outline: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GetArgs {
    dir: String,
    symbol_or_path: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GetResponse {
    dir: String,
    symbol_or_path: String,
    content: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SearchArgs {
    dir: String,
    query: String,
    #[serde(default)]
    expand: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SearchResponse {
    dir: String,
    query: String,
    results: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct MapArgs {
    dir: String,
    #[serde(default = "default_format")]
    format: String,
    max_tokens: Option<usize>,
}

fn default_format() -> String {
    "xml".to_string()
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GraphArgs {
    dir: String,
    #[serde(default = "default_markdown_format")]
    format: Option<String>,
    top: Option<usize>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GraphResponse {
    dir: String,
    content: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GraphFileArgs {
    dir: String,
    file: String,
    #[serde(default = "default_depth")]
    depth: Option<usize>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GraphFileResponse {
    dir: String,
    file: String,
    content: String,
}

fn default_markdown_format() -> Option<String> {
    Some("markdown".to_string())
}

fn default_depth() -> Option<usize> {
    Some(3)
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct MapResponse {
    dir: String,
    content: String,
}

#[derive(Debug, Clone)]
pub struct CodebonesMcpServer {
    tool_router: ToolRouter<Self>,
}

impl Default for CodebonesMcpServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router(router = tool_router)]
impl CodebonesMcpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    fn invalid_params(tool: &str, message: String, data: rmcp::serde_json::Value) -> ErrorData {
        ErrorData::invalid_params(format!("{tool} failed: {message}"), Some(data))
    }

    fn map_lookup_error(tool: &str, message: String, data: rmcp::serde_json::Value) -> ErrorData {
        if message.contains("not found") || message.contains("No such file or directory") {
            return ErrorData::resource_not_found(format!("{tool} failed: {message}"), Some(data));
        }

        if message.contains("invalid") {
            return ErrorData::invalid_params(format!("{tool} failed: {message}"), Some(data));
        }

        ErrorData::internal_error(format!("{tool} failed: {message}"), Some(data))
    }

    fn ensure_dir(tool: &str, dir: &str) -> Result<(), ErrorData> {
        let path = Path::new(dir);
        if !path.exists() {
            return Err(Self::invalid_params(
                tool,
                format!("directory does not exist: {dir}"),
                rmcp::serde_json::json!({
                    "tool": tool,
                    "dir": dir,
                }),
            ));
        }
        if !path.is_dir() {
            return Err(Self::invalid_params(
                tool,
                format!("path is not a directory: {dir}"),
                rmcp::serde_json::json!({
                    "tool": tool,
                    "dir": dir,
                }),
            ));
        }
        Ok(())
    }

    #[tool(
        name = "index",
        description = "Builds or updates the codebones index for a directory"
    )]
    async fn index(
        &self,
        Parameters(IndexArgs { dir }): Parameters<IndexArgs>,
    ) -> Result<Json<IndexResponse>, ErrorData> {
        Self::ensure_dir("index", &dir)?;
        let dir_path = Path::new(&dir);
        codebones_core::api::index(dir_path).map_err(|error| {
            ErrorData::internal_error(
                format!("index failed: {}", error),
                Some(rmcp::serde_json::json!({
                    "tool": "index",
                    "dir": dir,
                })),
            )
        })?;

        Ok(Json(IndexResponse {
            dir,
            status: "indexed".to_string(),
        }))
    }

    #[tool(
        name = "outline",
        description = "Gets the skeleton outline of an indexed file"
    )]
    async fn outline(
        &self,
        Parameters(OutlineArgs { dir, path }): Parameters<OutlineArgs>,
    ) -> Result<Json<OutlineResponse>, ErrorData> {
        Self::ensure_dir("outline", &dir)?;
        let outline = codebones_core::api::outline(Path::new(&dir), &path).map_err(|error| {
            Self::map_lookup_error(
                "outline",
                error.to_string(),
                rmcp::serde_json::json!({
                    "tool": "outline",
                    "dir": dir,
                    "path": path,
                }),
            )
        })?;

        Ok(Json(OutlineResponse { dir, path, outline }))
    }

    #[tool(
        name = "get",
        description = "Retrieves the full source code for a specific symbol or file"
    )]
    async fn get(
        &self,
        Parameters(GetArgs {
            dir,
            symbol_or_path,
        }): Parameters<GetArgs>,
    ) -> Result<Json<GetResponse>, ErrorData> {
        Self::ensure_dir("get", &dir)?;
        let content =
            codebones_core::api::get(Path::new(&dir), &symbol_or_path).map_err(|error| {
                Self::map_lookup_error(
                    "get",
                    error.to_string(),
                    rmcp::serde_json::json!({
                        "tool": "get",
                        "dir": dir,
                        "symbol_or_path": symbol_or_path,
                    }),
                )
            })?;

        Ok(Json(GetResponse {
            dir,
            symbol_or_path,
            content,
        }))
    }

    #[tool(
        name = "map",
        description = "Outputs the skeleton map only (file paths + symbol signatures) — shorthand for pack --no-files"
    )]
    async fn map(
        &self,
        Parameters(MapArgs {
            dir,
            format,
            max_tokens,
        }): Parameters<MapArgs>,
    ) -> Result<Json<MapResponse>, ErrorData> {
        Self::ensure_dir("map", &dir)?;
        let content = codebones_core::api::pack(
            Path::new(&dir),
            &format,
            max_tokens,
            codebones_core::api::PackOptions {
                no_file_summary: false,
                no_files: true,
                remove_comments: false,
                remove_empty_lines: false,
                truncate_base64: false,
                include: None,
                ignore: None,
            },
        )
        .map_err(|error| {
            Self::map_lookup_error(
                "map",
                error.to_string(),
                rmcp::serde_json::json!({
                    "tool": "map",
                    "dir": dir,
                }),
            )
        })?;

        Ok(Json(MapResponse { dir, content }))
    }

    #[tool(
        name = "graph",
        description = "Returns the full import dependency graph showing which files are most imported"
    )]
    async fn graph(
        &self,
        Parameters(GraphArgs { dir, format, top }): Parameters<GraphArgs>,
    ) -> Result<Json<GraphResponse>, ErrorData> {
        Self::ensure_dir("graph", &dir)?;
        let format_str = format.as_deref().unwrap_or("markdown");
        let mut result = codebones_core::api::graph(Path::new(&dir)).map_err(|error| {
            ErrorData::internal_error(
                format!("graph failed: {}", error),
                Some(rmcp::serde_json::json!({
                    "tool": "graph",
                    "dir": dir,
                })),
            )
        })?;

        if let Some(n) = top {
            result.files.truncate(n);
        }

        let content = format_graph_mcp(&result, format_str, top);
        Ok(Json(GraphResponse { dir, content }))
    }

    #[tool(
        name = "graph_file",
        description = "Returns the blast radius for a specific file: all files that transitively import it"
    )]
    async fn graph_file(
        &self,
        Parameters(GraphFileArgs { dir, file, depth }): Parameters<GraphFileArgs>,
    ) -> Result<Json<GraphFileResponse>, ErrorData> {
        Self::ensure_dir("graph_file", &dir)?;
        let max_depth = depth.unwrap_or(3);
        let result = codebones_core::api::graph_file(Path::new(&dir), &file, max_depth).map_err(
            |error| {
                ErrorData::internal_error(
                    format!("graph_file failed: {}", error),
                    Some(rmcp::serde_json::json!({
                        "tool": "graph_file",
                        "dir": dir,
                        "file": file,
                    })),
                )
            },
        )?;

        let content = format_blast_radius_mcp(&file, &result.affected_files);
        Ok(Json(GraphFileResponse { dir, file, content }))
    }

    #[tool(
        name = "search",
        description = "Searches for symbols across the repository. Pass expand: true to include full source for each match."
    )]
    async fn search(
        &self,
        Parameters(SearchArgs { dir, query, expand }): Parameters<SearchArgs>,
    ) -> Result<Json<SearchResponse>, ErrorData> {
        Self::ensure_dir("search", &dir)?;
        let results = if expand {
            codebones_core::api::search_expanded(Path::new(&dir), &query)
                .map_err(|error| {
                    Self::map_lookup_error(
                        "search",
                        error.to_string(),
                        rmcp::serde_json::json!({
                            "tool": "search",
                            "dir": dir,
                            "query": query,
                        }),
                    )
                })?
                .into_iter()
                .map(|r| format!("{}\n{}", r.id, r.source))
                .collect()
        } else {
            codebones_core::api::search(Path::new(&dir), &query).map_err(|error| {
                Self::map_lookup_error(
                    "search",
                    error.to_string(),
                    rmcp::serde_json::json!({
                        "tool": "search",
                        "dir": dir,
                        "query": query,
                    }),
                )
            })?
        };

        Ok(Json(SearchResponse {
            dir,
            query,
            results,
        }))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for CodebonesMcpServer {}
