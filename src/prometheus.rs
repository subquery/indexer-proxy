use once_cell::sync::Lazy;
use prometheus::{labels, register_int_counter_vec, IntCounterVec};

use crate::{account, cli::COMMAND};

pub static QUERY_COUNTER: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "subquery_indexer_query_total",
        "Total number of query request.",
        &["deployment_id"]
    )
    .unwrap()
});

fn pushgateway_url() -> String {
    let url = if COMMAND.dev() {
        "https://pushgateway-kong-dev.onfinality.me"
    } else {
        "https://pushgateway.subquery.network"
    };

    url.to_string()
}

pub fn push_query_metrics(id: String) {
    tokio::spawn(push_query_total(id));
}

pub async fn push_query_total(id: String) {
    let url = pushgateway_url();
    let indexer = account::get_indexer().await;

    QUERY_COUNTER.with_label_values(&[&id]).inc();

    let _ = prometheus::push_add_metrics(
        "subql_indexer_query",
        labels! {"instance".to_string() => indexer},
        &url,
        prometheus::gather(),
        None,
    );
}
