use blake3;
use lazy_static::lazy_static;
use reqwest::header::HeaderValue;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use tracing::info;
use tungstenite::client::IntoClientRequest;
use tungstenite::{connect, Message};

use crate::cli::CommandLineArgs;
use crate::error::Error;
use crate::request::{graphql_request, GraphQLQuery};

lazy_static! {
    pub static ref PROJECTS: Mutex<HashMap<String, String>> = Mutex::new(HashMap::new());
}

impl PROJECTS {
    pub fn add(deployment_id: String, url: String) {
        let mut map = PROJECTS.lock().unwrap();
        map.insert(deployment_id, url);
    }

    pub fn get(key: &str) -> Result<String, Error> {
        let map = PROJECTS.lock().unwrap();
        let url = match map.get(key) {
            Some(url) => url,
            None => return Err(Error::InvalidProejctId),
        };
        Ok(url.to_owned())
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct ProjectsResponse {
    #[serde(rename = "getAliveProjects")]
    get_alive_projects: Vec<ProjectItem>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ProjectItem {
    id: String,
    #[serde(rename = "queryEndpoint")]
    query_endpoint: String,
}

impl ProjectItem {
    fn hash(&self) -> String {
        blake3::hash(self.id.as_bytes()).to_string()
    }
}

pub async fn validate_service_url(url: &str) {
    let query_string = String::from("query { accountMetadata { indexer } }");
    let query = GraphQLQuery::new(query_string);
    let result = graphql_request(url, query).await;

    match result {
        Ok(value) => {
            let _ = value.pointer("/data/accountMetadata/indexer").unwrap();
        }
        Err(e) => panic!("Invalid service url: {}, error: {}", url, e),
    };
}

pub async fn init_projects(url: &str) {
    // graphql query for getting alive projects
    let query_string = String::from("query { getAliveProjects { id queryEndpoint } }");
    let query = GraphQLQuery::new(query_string);
    let result = graphql_request(url, query).await;

    match result {
        Ok(value) => {
            let v_d = value.pointer("/data").unwrap();
            let v_str = serde_json::to_string(v_d).unwrap();
            let v: ProjectsResponse = serde_json::from_str(v_str.as_str()).unwrap();
            for item in v.get_alive_projects {
                PROJECTS::add(item.hash(), item.query_endpoint);
            }
        }
        Err(e) => println!("Init projects failed: {}", e),
    };

    if CommandLineArgs::debug() {
        info!("indexing projects: {:?}", PROJECTS.lock().unwrap());
    }
}

pub fn subscribe_project_change(url: &str) {
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
        // info!("text: {:?}", incoming_msg.to_text());
        // let text = incoming_msg.to_text().unwrap();
        // let value: Value = serde_json::from_str(text).unwrap();
        info!("incoming_msg: {:?}", incoming_msg);
        // let project = value.pointer("payload/data/projectChanged").unwrap();
        // let item: ProjectItem = serde_json::from_str(project.to_string().as_str()).unwrap();
        // PROJECTS::add(item.hash(), item.query_endpoint);

        if CommandLineArgs::debug() {
            info!("indexing projects: {:?}", PROJECTS.lock().unwrap());
        }
    }
}
