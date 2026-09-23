//! Compiles and runs every Rust code block in the book (`book/src`) and the
//! root `README.md` as a doctest, so that they stay in sync with the API.
//! Run with `cargo test -p zestors-book --doc`.

#[cfg(doctest)]
mod doctests {
    macro_rules! chapters {
        ($($name:ident = $path:literal;)*) => {
            $(#[doc = include_str!($path)] mod $name {})*
        };
    }

    #[doc = include_str!("../README.md")]
    mod readme {}

    chapters! {
        introduction = "src/introduction.md";
        getting_started = "src/getting-started.md";
        messages = "src/messages.md";
        actors = "src/actors.md";
        lifecycle = "src/lifecycle.md";
        handler = "src/handler.md";
        dynamic_addresses = "src/dynamic-addresses.md";
        supervision = "src/supervision.md";
        observability = "src/observability.md";
        distributed_overview = "src/distributed/overview.md";
        distributed_remote_messages = "src/distributed/remote-messages.md";
        distributed_running_a_cluster = "src/distributed/running-a-cluster.md";
        distributed_addressing = "src/distributed/addressing.md";
        distributed_remote_requests = "src/distributed/remote-requests.md";
        distributed_monitoring = "src/distributed/monitoring.md";
        distributed_delivery = "src/distributed/delivery.md";
        distributed_membership = "src/distributed/membership.md";
        distributed_testing = "src/distributed/testing.md";
        distributed_backends = "src/distributed/backends.md";
        reference_derive_attributes = "src/reference/derive-attributes.md";
        reference_feature_flags = "src/reference/feature-flags.md";
    }
}
