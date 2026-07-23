//! Shared semantic call-summary vocabulary.
//!
//! WHAT: owns the backend-neutral parameter, effect, transfer, reactive and return-alias facts
//! shared by borrow validation and the declaration-centric public-interface draft.
//! WHY: both stages consume the same semantic contract. Keeping the vocabulary at the frontend
//! boundary prevents either stage from becoming the source of a second interpretation.

/// The source-level access contract for one function parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PublicCallParameterAccess {
    Shared,
    Mutable,
    Reactive,
}

/// The mutation effect observed for one parameter's root during borrow validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PublicCallMutationEffect {
    NoWrite,
    Writes,
}

/// Whether final-use analysis may grant optional transfer responsibility to one parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PublicCallTransferEligibility {
    Ineligible,
    Eligible,
}

/// The analysis/lowering transfer category for one parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PublicCallTransferEffect {
    NeverConsumes,
    MayConsume,
    /// Reserved for a specialised already-proven path. Ordinary local source calls remain
    /// optional and use `MayConsume` instead.
    #[allow(dead_code)]
    AlwaysConsumes,
}

/// Reactive dependency and invalidation facts for one parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PublicCallReactiveEffect {
    None,
    Subscribes,
    Invalidates,
    SubscribesAndInvalidates,
}

impl PublicCallReactiveEffect {
    pub(crate) fn with_subscription(self) -> Self {
        match self {
            Self::None | Self::Subscribes => Self::Subscribes,
            Self::Invalidates => Self::SubscribesAndInvalidates,
            Self::SubscribesAndInvalidates => Self::SubscribesAndInvalidates,
        }
    }

    pub(crate) fn with_invalidation(self) -> Self {
        match self {
            Self::None | Self::Invalidates => Self::Invalidates,
            Self::Subscribes => Self::SubscribesAndInvalidates,
            Self::SubscribesAndInvalidates => Self::SubscribesAndInvalidates,
        }
    }
}

/// Owned semantic facts for one parameter, retained in source parameter order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PublicCallParameterSummary {
    pub access: PublicCallParameterAccess,
    pub mutation: PublicCallMutationEffect,
    pub transfer_eligibility: PublicCallTransferEligibility,
    pub transfer_effect: PublicCallTransferEffect,
    pub reactive_effect: PublicCallReactiveEffect,
}

/// User-function return alias metadata consumed by call transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FunctionReturnAliasSummary {
    Fresh,
    AliasParams(Vec<usize>),
    Unknown,
}

/// Complete semantic call contract for one local or generated function.
///
/// Parameter positions use vector order and the indices in [`FunctionReturnAliasSummary`]. No
/// donor-local HIR identity crosses this frontend semantic boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PublicCallSummary {
    pub parameters: Vec<PublicCallParameterSummary>,
    pub return_alias: FunctionReturnAliasSummary,
}

/// State of one declaration's public call contract during direct-interface finalization.
///
/// `PendingLocal` exists only between AST draft construction and local HIR/borrow joining.
/// `PendingGenerated` intentionally applies to exported generic templates whose concrete
/// functions belong to the R3 sidecar worklist and therefore have no base local `FunctionId`.
/// Only `Finalized` may represent a non-generic local callable in a completed module result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PublicCallSummaryState {
    PendingLocal,
    PendingGenerated,
    Finalized(PublicCallSummary),
}
