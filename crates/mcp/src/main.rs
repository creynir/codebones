mod rmcp {
    pub mod server {
        pub struct Server;
        impl Server {
            pub fn new(_name: &str, _version: &str) -> Self { Server }
            pub fn register_tool<F, Fut>(&mut self, _tool: super::Tool, _handler: F)
            where
                F: Fn(String) -> Fut + Send + Sync + 'static,
                Fut: std::future::Future<Output = anyhow::Result<String>> + Send + 'static,
            {}
            pub async fn serve_stdio(&self) -> anyhow::Result<()> { Ok(()) }
        }
    }
    pub struct Tool;
    impl Tool {
        pub fn new(_name: &str, _desc: &str) -> Self { Tool }
        pub fn with_arg(self, _name: &str, _type: &str, _desc: &str) -> Self { self }
    }
}

use rmcp::{server::Server, Tool};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut server = Server::new("codebones-mcp", "0.1.0");

    server.register_tool(
        Tool::new("index", "Builds or updates the codebones index for a directory")
            .with_arg("dir", "string", "Directory to index"),
        |args| async move {
            codebones_core::api::index(std::path::Path::new(&args)).unwrap();
            Ok("Indexing complete".into())
        }
    );

    server.register_tool(
        Tool::new("outline", "Gets the skeleton outline of a file or directory")
            .with_arg("path", "string", "Path to file or directory"),
        |args| async move {
            let result = codebones_core::api::outline(std::path::Path::new("."), &args).unwrap_or_else(|e| e.to_string());
            Ok(result)
        }
    );

    server.register_tool(
        Tool::new("get", "Retrieves the full source code for a specific symbol")
            .with_arg("symbol", "string", "Symbol name to retrieve"),
        |args| async move {
            let result = codebones_core::api::get(std::path::Path::new("."), &args).unwrap_or_else(|e| e.to_string());
            Ok(result)
        }
    );

    server.register_tool(
        Tool::new("search", "Searches for symbols across the repository")
            .with_arg("query", "string", "Search query"),
        |args| async move {
            let results = codebones_core::api::search(std::path::Path::new("."), &args).unwrap_or_default();
            Ok(results.join("\n"))
        }
    );

    server.serve_stdio().await?;
    Ok(())
}
