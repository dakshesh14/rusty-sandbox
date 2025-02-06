pub mod apis;
pub mod app;
pub mod constants;
pub mod sandbox;

#[tokio::main]
async fn main() {
    app::run_server().await;
}
