//! JavaScript backend semantic correctness tests.
//!
//! These modules pin the observable contract between Beanstalk HIR semantics and emitted JS text.
//! They inspect generated source rather than executing JavaScript, keeping each backend concern in
//! a focused file and sharing direct-HIR construction through `support`.

mod support;

mod bindings;
mod choices;
mod control_flow;
mod expressions;
mod host;
mod prelude;
mod receiver_methods;
mod results;
mod runtime_helpers;
mod symbols;
