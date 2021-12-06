use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "Indexer Proxy",
    about = "Command line for starting indexer proxy server"
)]
pub struct CommandLineArgs {
    /// enable debug mode
    #[structopt(short = "d", long = "debug")]
    pub debug: bool,
    /// Port the service will listen on
    #[structopt(short = "p", long = "port", default_value = "8003")]
    pub port: u16,
    /// Coordinator service endpoint
    #[structopt(long = "service-url")]
    pub service_url: String,
    /// Secret key for generating auth token
    #[structopt(long = "secret-key")]
    pub secret_key: String,
}

impl CommandLineArgs {
    pub fn port() -> u16 {
        CommandLineArgs::from_args().port
    }

    pub fn service_url() -> String {
        CommandLineArgs::from_args().service_url
    }

    pub fn secret_key() -> String {
        CommandLineArgs::from_args().secret_key
    }

    pub fn debug() -> bool {
        CommandLineArgs::from_args().debug
    }
}
