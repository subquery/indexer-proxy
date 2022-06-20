// This file is part of SubQuery.

// Copyright (C) 2020-2022 SubQuery Pte Ltd authors & contributors
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use once_cell::sync::Lazy;
use secp256k1::SecretKey;
use serde_json::{json, Value};
use structopt::StructOpt;
use subql_proxy_utils::request::{jsonrpc_request, proxy_request};
use web3::{signing::SecretKeyRef, types::Address};

#[cfg(feature = "p2p")]
use subql_proxy_utils::p2p::libp2p::Multiaddr;

const SEED_ADDR: &'static str = "/ip4/0.0.0.0/tcp/7000";
const P2P_ADDR: &'static str = "/ip4/0.0.0.0/tcp/0";

pub static COMMAND: Lazy<CommandArgs> = Lazy::new(|| CommandLineArgs::from_args().parse());

pub enum IndexerNetwork {
    Url(String),
    P2p(Multiaddr),
}

impl IndexerNetwork {
    pub async fn open(&self, indexer: String, raw_state: String) -> Result<Value, Value> {
        match self {
            IndexerNetwork::Url(url) => proxy_request("post", url, "open", "", raw_state, vec![]).await,
            IndexerNetwork::P2p(addr) => {
                let data = json!({ "method": "open", "state": raw_state });
                let infos = serde_json::to_string(&data).unwrap();
                let query = vec![Value::from(indexer), Value::from(infos)];
                jsonrpc_request(0, "127.0.0.1:8011", "state-channel", query).await
            }
        }
    }
}

#[derive(Debug, StructOpt)]
#[structopt(name = "Consumer Proxy", about = "Command line for starting consumer proxy server")]
pub struct CommandLineArgs {
    /// IP address for the server
    #[structopt(long = "host", short = "h", default_value = "0.0.0.0")]
    pub host: String,
    /// Port the service will listen on
    #[structopt(long = "port", short = "p", default_value = "8010")]
    pub port: u16,
    /// Indexer service endpoint
    #[structopt(long = "indexer-url", short = "i")]
    pub indexer_url: Option<String>,
    /// Indexer service endpoint
    #[structopt(long = "indexer-p2p")]
    pub indexer_p2p: Option<Multiaddr>,
    /// Check if running p2p as relay.
    #[structopt(long = "relay")]
    pub relay: bool,
    /// Enable debug mode
    #[structopt(long = "debug")]
    pub debug: bool,
    /// Enable dev mode
    #[structopt(long = "dev")]
    pub dev: bool,
    /// Consumer proxy contract
    #[structopt(long = "contract")]
    pub contract: String,
    /// Signer secret key
    #[structopt(long = "signer")]
    pub signer: String,
}

impl CommandLineArgs {
    pub fn parse(self) -> CommandArgs {
        let indexer = if let Some(url) = self.indexer_url {
            IndexerNetwork::Url(url.clone())
        } else {
            IndexerNetwork::P2p(self.indexer_p2p.unwrap())
        };

        let p2p = if self.relay {
            SEED_ADDR.parse().unwrap()
        } else {
            P2P_ADDR.parse().unwrap()
        };

        CommandArgs {
            host: self.host,
            port: self.port,
            dev: self.dev,
            debug: self.debug,
            indexer: indexer,
            p2p: p2p,
            contract: self.contract.parse().unwrap(),
            signer: SecretKey::from_slice(&hex::decode(&self.signer).unwrap()).unwrap(),
        }
    }
}

pub struct CommandArgs {
    pub host: String,
    pub port: u16,
    pub debug: bool,
    pub dev: bool,
    pub p2p: Multiaddr,
    pub indexer: IndexerNetwork,
    pub contract: Address,
    pub signer: SecretKey,
}

#[allow(dead_code)]
impl CommandArgs {
    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn indexer(&self) -> &IndexerNetwork {
        &self.indexer
    }

    pub fn debug(&self) -> bool {
        self.debug
    }

    pub fn dev(&self) -> bool {
        self.dev
    }

    pub fn p2p(&self) -> Multiaddr {
        self.p2p.clone()
    }

    pub fn contract(&self) -> Address {
        self.contract
    }

    pub fn signer(&self) -> SecretKeyRef {
        SecretKeyRef::new(&self.signer)
    }
}
