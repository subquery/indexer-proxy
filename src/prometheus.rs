use lazy_static::lazy_static;
use prometheus::{labels, register_int_counter_vec, IntCounterVec};

use crate::{account, cli};

lazy_static! {
    static ref QUERY_COUNTER: IntCounterVec = register_int_counter_vec!(
        "subquery_indexer_query_total",
        "Total number of query request.",
        &["indexer", "deployment_id"]
    )
    .unwrap();
}

pub fn push_query_metrics(id: String) {
    std::thread::spawn(move || {
        push_query_total(&id);
    });
}

pub fn push_query_total(id: &str) {
    let url = cli::CommandLineArgs::pushgateway_url();
    let indexer = account::get_indexer();

    QUERY_COUNTER.with_label_values(&[&indexer, id]).inc();

    let _ = prometheus::push_metrics(
        "subql_indexer_query",
        labels!{},
        &url,
        prometheus::gather(),
        None,
    );
}
