use lazy_static::lazy_static;
use std::net::SocketAddr;
use structopt::StructOpt;

#[cfg(feature = "p2p")]
use libp2p::Multiaddr;

#[cfg(feature = "p2p")]
const SEED_ADDR: &'static str = "/ip4/0.0.0.0/tcp/7000";
#[cfg(feature = "p2p")]
const P2P_ADDR: &'static str = "/ip4/0.0.0.0/tcp/0";

lazy_static! {
    pub static ref COMMAND: CommandLineArgs = CommandLineArgs::new();
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = "Indexer Proxy",
    about = "Command line for starting indexer proxy server"
)]

pub struct CommandLineArgs {
    /// Port the service will listen on
    #[structopt(short = "p", long = "port", default_value = "8003")]
    pub port: u16,
    /// Coordinator service endpoint
    #[structopt(long = "service-url")]
    pub service_url: String,
    /// Secret key for generating auth token
    #[structopt(long = "secret-key")]
    pub secret_key: String,
    /// IP address for the server
    #[structopt(long = "host", default_value = "127.0.0.1")]
    pub host: String,
    /// enable auth
    #[structopt(short = "a", long = "auth")]
    pub auth: bool,
    /// enable debug mode
    #[structopt(short = "d", long = "debug")]
    pub debug: bool,
    /// enable dev mode
    #[structopt(long = "dev")]
    pub dev: bool,
    /// Rpc binding socket address.
    #[structopt(short = "r", long = "p2p-rpc", default_value = "127.0.0.1:7001")]
    pub p2p_rpc: SocketAddr,
    /// Rpc binding socket address.
    #[structopt(short = "w", long = "p2p-ws")]
    pub p2p_ws: Option<SocketAddr>,
    /// Check if running as relay.
    #[structopt(short = "e", long = "p2p-relay")]
    pub p2p_relay: bool,
}

impl CommandLineArgs {
    pub fn new() -> Self {
        CommandLineArgs::from_args()
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn service_url(&self) -> String {
        self.service_url.clone()
    }

    // pub fn secret_key(&self) -> String {
    //     self.secret_key
    // }

    pub fn host(&self) -> String {
        self.host.clone()
    }

    pub fn debug(&self) -> bool {
        self.debug
    }

    pub fn auth(&self) -> bool {
        self.auth
    }

    pub fn dev(&self) -> bool {
        self.dev
    }

    pub fn rpc(&self) -> SocketAddr {
        self.p2p_rpc
    }

    pub fn ws(&self) -> Option<SocketAddr> {
        self.p2p_ws
    }

    #[cfg(feature = "p2p")]
    pub fn p2p(&self) -> Multiaddr {
        if self.p2p_relay {
            SEED_ADDR.parse().unwrap()
        } else {
            P2P_ADDR.parse().unwrap()
        }
    }
}
