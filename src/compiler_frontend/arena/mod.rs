//! Frontend arena policy and capacity-estimate types.
//!
//! WHAT: gathers cheap, unconditional token and header statistics during existing frontend
//!       passes, then turns those facts into conservative capacity estimates for future typed
//!       `Vec` arenas.
//! WHY: arena capacity is tunable policy, not correctness logic. Keeping the stats and estimates
//!      in one stage-local module prevents pipeline files from becoming bloated with heuristic
//!      formulas and counter wiring.
//!
//! MUST NOT OWN:
//! - actual arena storage or ID-handle allocation;
//! - scope-context or expression lowering behavior;
//! - string-table identity or remap logic (stats contain counts only).

pub(crate) mod capacity;
pub(crate) mod header_stats;
pub(crate) mod token_stats;

pub(crate) use capacity::FrontendArenaCapacityEstimate;
pub(crate) use header_stats::HeaderStats;
pub(crate) use token_stats::TokenStats;

#[cfg(test)]
mod tests;
