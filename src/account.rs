use lazy_static::lazy_static;
use serde_json::json;
use std::sync::Mutex;

use crate::cli::CommandLineArgs;
use crate::request::graphql_request;
use crate::{error::Error, types::Result};

// pub static mut INDEXER: Option<String> = None;

pub struct Account {
    indexer: Option<String>,
}

lazy_static! {
    pub static ref ACCOUNT: Mutex<Account> = Mutex::new(Account { indexer: None });
}

pub async fn fetch_account_metadata() -> Result<String> {
    let url = CommandLineArgs::service_url();
    let query = json!({"query": "query { accountMetadata { indexer } }" });
    let result = graphql_request(&url, &query).await;

    let indexer = match result {
        Ok(value) => match value.pointer("/data/accountMetadata/indexer") {
            Some(v_d) => serde_json::to_string(v_d).unwrap_or(String::from("")),
            None => return Err(Error::InvalidServiceEndpoint),
        },
        Err(_) => return Err(Error::InvalidServiceEndpoint),
    };

    if !indexer.is_empty() {
        ACCOUNT.lock().unwrap().indexer = Some(indexer.to_owned());
    }

    Ok(indexer)
}

pub fn update_account_metadata() {
    let account = ACCOUNT.lock().unwrap();
    if !account.indexer.is_some() {
        let _ = fetch_account_metadata();
    }
}

pub fn get_indexer() -> String {
    let account = ACCOUNT.lock().unwrap();
    if account.indexer.is_some() {
        return account.indexer.to_owned().unwrap();
    }

    return String::from("default_indexer");
}
