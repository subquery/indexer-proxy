mod auth;
mod cli;
mod constants;
mod error;
mod project;
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

    tracing_subscriber::fmt().init();
    project::validate_service_url(&service_url).await;
    project::init_projects(&service_url).await;
    project::subscribe();
    server::start_server(&host, port).await;
}
