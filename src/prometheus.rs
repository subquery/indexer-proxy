use lazy_static::lazy_static;
use prometheus::{labels, register_counter, Counter};
use std::thread;

use crate::cli;

lazy_static! {
    static ref PUSH_COUNTER: Counter = register_counter!(
        "subql_network_query_total",
        "Total number of query request."
    )
    .unwrap();
}

pub fn push_query_metrics() {
    thread::spawn(move || {
        push_query_total();
    });
}

fn push_query_total() {
    let url = cli::CommandLineArgs::pushgateway_url();

    PUSH_COUNTER.inc();
    prometheus::push_metrics(
        "subql_network_query",
        labels! {"indexer".to_owned() => "0xregfdgade34f".to_owned()},
        &url,
        prometheus::gather(),
        None,
    )
    .unwrap();
}
