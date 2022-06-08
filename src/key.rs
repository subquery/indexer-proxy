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
use reqwest::header::HeaderValue;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use std::thread;
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{connect, Message};

use secp256k1::{SecretKey, ONE_KEY};
use web3::{Address, SecretKeyRef};

use crate::account;
use crate::cli::COMMAND;
use crate::error::Error;
use crate::request::graphql_request;

pub struct Key {
    pub indexer: Address,
    pub controller: Address,
    pub controller_sk: SecretKey,
}

impl Default for Key {
    fn default() -> Self {
        let controller_sk = ONE_KEY;
        let controller = SecretKeyRef::new(&controller_sk).address();
        Key {
            indexer: Address::default(),
            controller,
            controller_sk,
        }
    }
}

pub static KEY: Lazy<RwLock<Key>> = Lazy::new(|| RwLock::new(Key::default()));

impl KEY {
    pub async fn update(indexer: Address, controller_sk: SecretKey) {
        let controller = SecretKeyRef::new(&controller_sk).address();
        let new_key = Key {
            indexer,
            controller,
            controller_sk,
        };
        let mut key = KEY.write().await;
        *key = new_key;
        drop(key);
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct KeyResponse {
    #[serde(rename = "getKey")]
    get_alive_projects: Vec<ProjectItem>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ProjectItem {
    id: String,
    #[serde(rename = "queryEndpoint")]
    query_endpoint: String,
}

pub async fn validate_service_url() {
    match account::fetch_account_metadata().await {
        Ok(_) => info!("Connect with coordinator service successfully"),
        Err(e) => panic!("Invalid coordinator service url with error: {}", e),
    }
}

pub async fn init_projects(url: &str) {
    // graphql query for getting alive projects
    let query = json!({ "query": "query { getAliveProjects { id queryEndpoint } }" });
    let result = graphql_request(url, &query).await;

    match result {
        Ok(value) => match value.pointer("/data") {
            Some(v_d) => {
                let v_str: String = serde_json::to_string(v_d).unwrap_or(String::from(""));
                let v: ProjectsResponse = serde_json::from_str(v_str.as_str()).unwrap();
                for item in v.get_alive_projects {
                    PROJECTS::add(item.id, item.query_endpoint);
                }
            }
            _ => {}
        },
        Err(e) => println!("Init projects failed: {}", e),
    };

    debug!("indexing projects: {:?}", PROJECTS.lock().unwrap());
}

pub fn subscribe() {
    thread::spawn(move || {
        let url = COMMAND.service_url();
        subscribe_project_change(url.as_str());
    });
}

fn subscribe_project_change(url: &str) {
    let mut websocket_url = url.to_owned();
    websocket_url.replace_range(0..4, "ws");

    let mut request = websocket_url.into_client_request().unwrap();
    request.headers_mut().insert(
        "Sec-WebSocket-Protocol",
        HeaderValue::from_str("graphql-ws").unwrap(),
    );
    let (mut socket, _) = connect(request).unwrap();
    info!("Connected to the websocket server");

    let out_message = json!({
        "type": "start",
        "payload": {
            "query": "subscription { projectChanged { id queryEndpoint } }"
        }
    })
    .to_string();
    let _ = socket.write_message(Message::Text(out_message)).unwrap();
    loop {
        let incoming_msg = socket.read_message().expect("Error reading message");
        let text = incoming_msg.to_text().unwrap();
        let value: Value = serde_json::from_str(text).unwrap();
        let project = value.pointer("/payload/data/projectChanged").unwrap();
        let item: ProjectItem = serde_json::from_str(project.to_string().as_str()).unwrap();
        PROJECTS::add(item.id, item.query_endpoint);

        debug!("indexing projects: {:?}", PROJECTS.lock().unwrap());
    }
}
