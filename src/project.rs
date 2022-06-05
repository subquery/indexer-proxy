use once_cell::sync::Lazy;
use reqwest::header::HeaderValue;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use std::thread;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{connect, Message};

use crate::cli::COMMAND;
use crate::error::Error;
use crate::request::graphql_request;

pub static PROJECTS: Lazy<Mutex<HashMap<String, String>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn add_project(deployment_id: String, url: String) {
    let mut map = PROJECTS.lock().unwrap();
    map.insert(deployment_id, url);
}

pub fn get_project(key: &str) -> Result<String, Error> {
    let map = PROJECTS.lock().unwrap();
    let url = match map.get(key) {
        Some(url) => url,
        None => return Err(Error::InvalidProejctId),
    };
    Ok(url.to_owned())
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

pub async fn init_projects() {
    let url = COMMAND.service_url();
    // graphql query for getting alive projects
    let query = json!({ "query": "query { getAliveProjects { id queryEndpoint } }" });
    let result = graphql_request(&url, &query).await;

    match result {
        Ok(value) => match value.pointer("/data") {
            Some(v_d) => {
                let v_str: String = serde_json::to_string(v_d).unwrap_or(String::from(""));
                let v: ProjectsResponse = serde_json::from_str(v_str.as_str()).unwrap();
                for item in v.get_alive_projects {
                    add_project(item.id, item.query_endpoint);
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
        subscribe_project_change(COMMAND.service_url());
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
        add_project(item.id, item.query_endpoint);

        debug!("indexing projects: {:?}", PROJECTS.lock().unwrap());
    }
}
