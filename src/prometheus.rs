use lazy_static::lazy_static;
use prometheus::{labels, register_counter, Counter};

use crate::{account, cli};

lazy_static! {
    static ref QUERY_COUNTER: Counter = register_counter!(
        "subql_network_query_total",
        "Total number of query request."
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

    let groupings = labels! {
        "indexer".to_owned() => indexer.to_owned(),
        "deployment_id".to_owned() => id.to_string().to_owned(),
    };

    QUERY_COUNTER.inc();
    let _ = prometheus::push_metrics(
        "subql_network_query",
        groupings,
        &url,
        prometheus::gather(),
        None,
    );
}
