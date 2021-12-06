#![allow(non_snake_case)]

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

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

    pub fn get(deployment_id: &str) -> Result<String, Error> {
        let map = PROJECTS.lock().unwrap();
        let url = match map.get(deployment_id) {
            Some(url) => url,
            None => return Err(Error::InvalidProejctId),
        };
        Ok(url.to_owned())
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Response {
    getAliveProjects: Vec<ProjectItem>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ProjectItem {
    id: String,
    queryEndpoint: String,
}

pub async fn init_projects(url: &str) {
    // graphql query for getting alive projects
    let query_string = String::from("query { getAliveProjects { id queryEndpoint } }");
    let query = GraphQLQuery::new(query_string);
    let result = graphql_request(url, query).await;

    match result {
        Ok(value) => {
            // TODO: error handling for desctructing | also extract these to a separate function | will use for subscription update
            let v_d = value.pointer("/data").unwrap();
            let v_str = serde_json::to_string(v_d).unwrap();
            let v: Response = serde_json::from_str(v_str.as_str()).unwrap();
            for item in v.getAliveProjects {
                PROJECTS::add(item.id, item.queryEndpoint);
            }
        }
        Err(e) => println!("{}", e),
    };
    println!("\n project results: {:?}", PROJECTS.lock().unwrap());
}
