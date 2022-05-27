#[macro_use]
extern crate tracing;

mod account;
mod auth;
mod cli;
mod constants;
mod eip712;
mod error;
mod payg;
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
    let port = COMMAND.port();
    let host = COMMAND.host();
    let debug = COMMAND.debug();

    let log_filter = if debug { Level::DEBUG } else { Level::INFO };
    tracing_subscriber::fmt().with_max_level(log_filter).init();

    account::fetch_account_metadata().await.unwrap();
    project::init_projects().await;

    project::subscribe();

    #[cfg(feature = "p2p")]
    {
        let p2p_bind = COMMAND.p2p();
        let p2p_rpc = COMMAND.rpc();
        let p2p_ws = COMMAND.ws();
        info!("P2P bind: {}", p2p_bind);

        let key_path = std::path::PathBuf::from("indexer.key"); // DEBUG TODO
        let key = if key_path.exists() {
            let key_bytes = tokio::fs::read(&key_path).await.unwrap_or(vec![]); // safe.
            libp2p::identity::Keypair::from_protobuf_encoding(&key_bytes).unwrap()
        } else {
            let key = libp2p::identity::Keypair::generate_ed25519();
            let _ = tokio::fs::write(key_path, key.to_protobuf_encoding().unwrap()).await;
            key
        };
        tokio::spawn(async move {
            p2p::server::server(p2p_bind, p2p_rpc, p2p_ws, key)
                .await
                .unwrap();
        });
    }

    server::start_server(host, port).await;
}
