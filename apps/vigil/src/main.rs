use tracing_subscriber;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    tracing::info!("CRYPTEK VIGIL v2.0 — Dynasty Wealth Monitoring Protocol");
    tracing::info!("Core: {}", vigil_core::hello());
}
