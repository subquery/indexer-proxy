mod auth;
mod constants;
mod error;
mod project;
mod request;
mod server;
mod types;

#[tokio::main]
async fn main() {
    // FIXME: get port from cli
    let url = "http://localhost:8000/graphql";
    let port = 8000;

    project::init_projects(url).await;
    server::start_server(port).await
}
