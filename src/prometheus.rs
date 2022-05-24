use lazy_static::lazy_static;
use prometheus::{labels, register_int_counter_vec, IntCounterVec};

use crate::{account, cli::COMMAND};

lazy_static! {
    static ref QUERY_COUNTER: IntCounterVec = register_int_counter_vec!(
        "subquery_indexer_query_total",
        "Total number of query request.",
        &["deployment_id"]
    )
    .unwrap();
}

fn pushgateway_url() -> String {
    let url = if COMMAND.dev() {
        "https://pushgateway-kong-dev.onfinality.me"
    } else {
        "https://pushgateway.subquery.network"
    };

    url.to_string()
}

pub fn push_query_metrics(id: String) {
    std::thread::spawn(move || {
        push_query_total(&id);
    });
}

pub fn push_query_total(id: &str) {
    let url = pushgateway_url();
    let indexer = account::get_indexer();

    QUERY_COUNTER.with_label_values(&[id]).inc();

    let _ = prometheus::push_add_metrics(
        "subql_indexer_query",
        labels! {"instance".to_string() => indexer},
        &url,
        prometheus::gather(),
        None,
    );
}
