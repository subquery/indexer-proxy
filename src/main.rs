mod auth;
mod cli;
mod constants;
mod error;
mod project;
mod request;
mod server;
mod types;

#[tokio::main]
async fn main() {
    let service_url = cli::CommandLineArgs::service_url();
    let port = cli::CommandLineArgs::port();

    project::validate_service_url(&service_url).await;
    project::init_projects(&service_url).await;
    server::start_server(port).await
}
