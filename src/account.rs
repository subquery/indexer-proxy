use lazy_static::lazy_static;
use secp256k1::{SecretKey, ONE_KEY};
use serde_json::json;
use std::sync::Mutex;
use tracing::info;
use web3::{
    signing::{Key, SecretKeyRef},
    types::Address,
};

use crate::cli::COMMAND;
use crate::request::graphql_request;
use crate::{error::Error, types::Result};

pub struct Account {
    pub indexer: Address,
    pub controller: Address,
    pub controller_sk: SecretKey,
}

impl Default for Account {
    fn default() -> Self {
        let controller_sk = ONE_KEY;
        let controller = SecretKeyRef::new(&controller_sk).address();
        Self {
            indexer: Address::default(),
            controller,
            controller_sk,
        }
    }
}

lazy_static! {
    pub static ref ACCOUNT: Mutex<Account> = Mutex::new(Account::default());
}

pub async fn fetch_account_metadata() -> Result<()> {
    let url = COMMAND.service_url();
    let query = json!({"query": "query { accountMetadata { indexer controller } }" });
    let result = graphql_request(&url, &query).await;
    let value = result.map_err(|_e| Error::InvalidServiceEndpoint)?;
    let indexer: Address = value
        .pointer("/data/accountMetadata/indexer")
        .ok_or(Error::InvalidServiceEndpoint)?
        .as_str()
        .unwrap_or("")
        .trim()
        .parse()
        .map_err(|_e| Error::InvalidServiceEndpoint)?;

    // let sk = value
    //     .pointer("/data/accountMetadata/controller")
    //     .ok_or(Error::InvalidServiceEndpoint)?
    //     .as_str()
    //     .unwrap_or("")
    //     .trim();
    let sk = "689af8efa8c651a91ad287602527f3af2fe9f6501a7ac4b061667b5a93e037fd"; // MOCK

    let controller_sk =
        SecretKey::from_slice(&hex::decode(sk).map_err(|_e| Error::InvalidServiceEndpoint)?)
            .map_err(|_e| Error::InvalidServiceEndpoint)?;

    let controller = SecretKeyRef::new(&controller_sk).address();
    info!("indexer: {:?}, controller: {:?}", indexer, controller);

    let new_account = Account {
        indexer,
        controller,
        controller_sk,
    };
    let mut account = ACCOUNT.lock().unwrap();
    *account = new_account;

    Ok(())
}

pub fn get_indexer() -> String {
    let account = ACCOUNT.lock().unwrap();
    format!("{:?}", account.indexer)
}

pub fn sign_message(_msg: &[u8]) -> String {
    //
    todo!()
}
