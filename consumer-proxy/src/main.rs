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

#[macro_use]
extern crate tracing;

mod cli;
mod payg;
mod server;

#[cfg(feature = "p2p")]
mod p2p;

use cli::COMMAND;
use tracing::Level;

#[cfg(feature = "p2p")]
use subql_proxy_utils::p2p::{libp2p::identity::Keypair, server::server as p2p_server};

#[tokio::main]
async fn main() {
    let log_filter = if COMMAND.debug() { Level::DEBUG } else { Level::INFO };
    tracing_subscriber::fmt().with_max_level(log_filter).init();

    #[cfg(feature = "p2p")]
    {
        let p2p_bind = COMMAND.p2p();
        info!("P2P bind: {}", p2p_bind);

        let key_path = std::path::PathBuf::from("indexer.key");
        let key = if key_path.exists() {
            let key_bytes = tokio::fs::read(&key_path).await.unwrap_or(vec![]); // safe.
            Keypair::from_protobuf_encoding(&key_bytes).unwrap()
        } else {
            let key = Keypair::generate_ed25519();
            let _ = tokio::fs::write(key_path, key.to_protobuf_encoding().unwrap()).await;
            key
        };
        tokio::spawn(async move {
            p2p_server::<p2p::ConsumerP2p>(p2p_bind, "127.0.0.1:8011".parse().unwrap(), None, None, key)
                .await
                .unwrap();
        });
    }

    // TODO listen the contract updated.

    server::start_server(COMMAND.host(), COMMAND.port()).await;
}
