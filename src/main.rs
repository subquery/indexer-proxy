use tracing::Level;

mod account;
mod auth;
mod cli;
mod constants;
mod eip712;
mod error;
mod project;
mod prometheus;
mod query;
mod request;
mod server;
mod tools;
mod traits;
mod types;

#[tokio::main]
async fn main() {
    let service_url = cli::CommandLineArgs::service_url();
    let port = cli::CommandLineArgs::port();
    let host = cli::CommandLineArgs::host();
    let debug = cli::CommandLineArgs::debug();

    let log_filter = if debug { Level::DEBUG } else { Level::INFO };
    tracing_subscriber::fmt().with_max_level(log_filter).init();

    project::validate_service_url().await;
    project::init_projects(&service_url).await;

    project::subscribe();
    server::start_server(&host, port).await;
}
