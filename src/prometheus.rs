// This file is part of SubQuery.

// Copyright (C) 2020-2022 SubQuery Pte Ltd authors & contributors
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

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
