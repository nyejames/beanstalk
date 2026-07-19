//! Self-tests grouped by integration test runner ownership.
//!
//! Each child module keeps the helpers and assertions for one runner concern
//! close to the code it protects.

mod assertions;
mod execution;
mod expectations;
mod fixture;
mod manifest;
mod reporting;
mod selection;
