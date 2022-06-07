use once_cell::sync::Lazy;
use secp256k1::{SecretKey, ONE_KEY};
use serde_json::json;
use tokio::sync::RwLock;
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

pub static ACCOUNT: Lazy<RwLock<Account>> = Lazy::new(|| RwLock::new(Account::default()));

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

    let sk = value
        .pointer("/data/accountMetadata/controller")
        .ok_or(Error::InvalidController)?
        .as_str()
        .unwrap_or("")
        .trim();
    let sk_values =
        serde_json::from_str::<serde_json::Value>(&sk).map_err(|_e| Error::InvalidController)?;
    if sk_values.get("iv").is_none() || sk_values.get("content").is_none() {
        return Err(Error::InvalidController);
    }
    let sk = COMMAND.decrypt(
        sk_values["iv"].as_str().ok_or(Error::InvalidController)?,
        sk_values["content"]
            .as_str()
            .ok_or(Error::InvalidController)?,
    )?; // with 0x...

    let controller_sk =
        SecretKey::from_slice(&hex::decode(&sk[2..]).map_err(|_e| Error::InvalidController)?)
            .map_err(|_e| Error::InvalidController)?;

    let controller = SecretKeyRef::new(&controller_sk).address();
    info!("indexer: {:?}, controller: {:?}", indexer, controller);

    let new_account = Account {
        indexer,
        controller,
        controller_sk,
    };
    let mut account = ACCOUNT.write().await;
    *account = new_account;

    Ok(())
}

pub async fn get_indexer() -> String {
    format!("{:?}", ACCOUNT.read().await.indexer)
}

pub fn sign_message(_msg: &[u8]) -> String {
    // TODO sign message to prove the result.
    "".to_owned()
}
