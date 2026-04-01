mod server;

use rmcp::ServiceExt;
use server::CodebonesMcpServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = CodebonesMcpServer::new()
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
