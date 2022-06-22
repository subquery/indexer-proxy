use once_cell::sync::Lazy;
use std::collections::HashMap;
use subql_proxy_utils::{
    error::Error,
    payg::{convert_sign_to_string, convert_string_to_sign, default_sign, OpenState, QueryState},
};
use tokio::sync::RwLock;
use web3::{
    signing::{SecretKeyRef, Signature},
    types::{Address, U256},
};

pub static CHANNELS: Lazy<RwLock<HashMap<U256, StateChannel>>> = Lazy::new(|| RwLock::new(HashMap::new()));
pub static PROJECTS: Lazy<RwLock<HashMap<String, U256>>> = Lazy::new(|| RwLock::new(HashMap::new()));

pub async fn get_project(project: &str) -> Result<U256, Error> {
    PROJECTS.read().await.get(project).cloned().ok_or(Error::InvalidRequest)
}

pub async fn add_project(project: String, channel: U256) {
    PROJECTS.write().await.insert(project, channel);
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
enum ChannelStatus {
    Finalized,
    Open,
    Challenge,
}

pub struct StateChannel {
    pub id: U256,
    status: ChannelStatus,
    indexer: Address,
    consumer: Address,
    current_count: U256,
    onchain_count: U256,
    remote_count: U256,
    balance: U256,
    expiration_at: U256,
    challenge_at: U256,
    last_final: bool,
    last_price: U256,
    last_indexer_sign: Signature,
    last_consumer_sign: Signature,
}

impl StateChannel {
    pub async fn get(project: &str) -> Result<StateChannel, Error> {
        let id = get_project(project).await?;
        let channel = CHANNELS.read().await.get(&id).cloned().ok_or(Error::InvalidRequest)?;
        Ok(channel)
    }

    pub async fn add(state: OpenState) {
        let channel = StateChannel {
            id: state.channel_id,
            indexer: state.indexer,
            consumer: state.consumer,
            balance: state.amount,
            expiration_at: state.expiration,
            status: ChannelStatus::Open,
            current_count: U256::from(0u64),
            onchain_count: U256::from(0u64),
            remote_count: U256::from(0u64),
            challenge_at: U256::from(0u64),
            last_price: state.next_price,
            last_final: false,
            last_indexer_sign: default_sign(),
            last_consumer_sign: default_sign(),
        };

        CHANNELS.write().await.insert(state.channel_id, channel);
    }

    pub fn next_query(self, sk: SecretKeyRef) -> Result<QueryState, Error> {
        let is_final = false; // TODO more
        let count = self.current_count + 1;

        QueryState::consumer_generate(
            self.id,
            self.indexer,
            self.consumer,
            count,
            self.last_price,
            is_final,
            sk,
        )
    }

    pub async fn renew(id: U256, state: QueryState) {
        if let Some(channel) = CHANNELS.write().await.get_mut(&id) {
            // TODO if next_price != last_price, checkpoint chain.
            // TODO adjust the count number if current_count != remote_count.

            channel.current_count = state.count;
            channel.remote_count = state.count;
            channel.last_price = state.next_price;
            channel.last_final = state.is_final;
            channel.last_indexer_sign = state.indexer_sign;
            channel.last_consumer_sign = state.consumer_sign;
        }
    }
}

impl Clone for StateChannel {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            status: self.status,
            indexer: self.indexer,
            consumer: self.consumer,
            current_count: self.current_count,
            onchain_count: self.onchain_count,
            remote_count: self.remote_count,
            balance: self.balance,
            expiration_at: self.expiration_at,
            challenge_at: self.challenge_at,
            last_final: self.last_final,
            last_price: self.last_price,
            last_indexer_sign: convert_string_to_sign(&convert_sign_to_string(&self.last_indexer_sign)),
            last_consumer_sign: convert_string_to_sign(&convert_sign_to_string(&self.last_consumer_sign)),
        }
    }
}
