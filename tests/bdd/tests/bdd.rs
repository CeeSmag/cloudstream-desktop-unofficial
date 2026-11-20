//! Cucumber BDD test entry point for CloudStream core user journeys.
#![allow(
    unreachable_pub,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::too_many_arguments
)]

mod harness;

use cucumber::World as _;
use harness::support::AppWorld;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    AppWorld::cucumber()
        .fail_on_skipped()
        .run_and_exit("features")
        .await;
}
