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

pub fn push_query_metrics() {
    std::thread::spawn(move || {
        push_query_total();
    });
}

pub fn push_query_total() {
    let url = cli::CommandLineArgs::pushgateway_url();
    let indexer = account::get_indexer();

    QUERY_COUNTER.inc();
    prometheus::push_metrics(
        "subql_network_query",
        labels! {"indexer".to_owned() => indexer.to_owned()},
        &url,
        prometheus::gather(),
        None,
    )
    .unwrap();
}
