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
use openssl::symm::{decrypt, Cipher};
use std::net::SocketAddr;
use structopt::StructOpt;

use crate::error::Error;

#[cfg(feature = "p2p")]
use libp2p::Multiaddr;

#[cfg(feature = "p2p")]
const SEED_ADDR: &'static str = "/ip4/0.0.0.0/tcp/7000";
#[cfg(feature = "p2p")]
const P2P_ADDR: &'static str = "/ip4/0.0.0.0/tcp/0";

pub static COMMAND: Lazy<CommandLineArgs> = Lazy::new(|| CommandLineArgs::from_args());

#[derive(Debug, StructOpt)]
#[structopt(name = "Indexer Proxy", about = "Command line for starting indexer proxy server")]

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
    /// auth token duration
    #[structopt(long = "token-duration", default_value = "12")]
    pub token_duration: i64,
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
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn service_url(&self) -> &str {
        &self.service_url
    }

    pub fn decrypt(&self, iv: &str, ciphertext: &str) -> Result<String, Error> {
        let iv = hex::decode(iv).map_err(|_| Error::InvalidEncrypt)?;
        let ctext = hex::decode(ciphertext).map_err(|_| Error::InvalidEncrypt)?;

        let ptext = decrypt(Cipher::aes_256_ctr(), self.secret_key.as_bytes(), Some(&iv), &ctext)
            .map_err(|_| Error::InvalidEncrypt)?;

        String::from_utf8(ptext.clone()).map_err(|_| Error::InvalidEncrypt)
    }

    pub fn host(&self) -> &str {
        &self.host
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

    pub fn token_duration(&self) -> i64 {
        self.token_duration
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
