use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    silo::run().await
}
