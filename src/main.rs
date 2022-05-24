#[macro_use]
extern crate tracing;

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

#[cfg(feature = "p2p")]
mod p2p;

use cli::COMMAND;
use tracing::Level;

#[tokio::main]
async fn main() {
    let service_url = COMMAND.service_url();
    let port = COMMAND.port();
    let host = COMMAND.host();
    let debug = COMMAND.debug();

    let log_filter = if debug { Level::DEBUG } else { Level::INFO };
    tracing_subscriber::fmt().with_max_level(log_filter).init();

    project::validate_service_url().await;
    project::init_projects(&service_url).await;

    project::subscribe();

    #[cfg(feature = "p2p")]
    {
        let p2p_bind = COMMAND.p2p();
        let p2p_rpc = COMMAND.rpc();
        let p2p_ws = COMMAND.ws();

        tokio::spawn(async move {
            let _ = p2p::server::server(
                p2p_bind,
                p2p_rpc,
                p2p_ws,
                std::path::PathBuf::from("indexer.key"), // DEBUG TODO
            )
            .await;
        });
    }

    tokio::spawn(server::start_server(host, port));
}
