//! Cross-case policy evaluation for the integration test suite.
//!
//! WHAT: evaluates ownership and assertion-strength rules after typed fixture loading.
//! WHY: cross-case policy must be decided once before reporting, selection or execution.

use super::{CaseRole, DiagnosticMatchMode, ExpectedOutcome, SuccessContract, TestSuiteSpec};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PolicyEvaluation {
    pub(crate) hard_findings: Vec<PolicyFinding>,
    pub(crate) advisories: Vec<PolicyFinding>,
}

impl PolicyEvaluation {
    pub(crate) fn has_hard_findings(&self) -> bool {
        !self.hard_findings.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PolicyFinding {
    pub(crate) code: String,
    pub(crate) case_id: Option<String>,
    pub(crate) message: String,
    #[serde(skip)]
    sort_key: FindingSortKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
struct FindingSortKey {
    case_order: usize,
    rule_order: usize,
    backend_order: usize,
}

struct CaseGroup {
    case_id: String,
    first_case_order: usize,
    case_orders: Vec<usize>,
}

/// A contract family aggregated across every canonical case that shares one contract.
///
/// `first_case_order` is the manifest order of the first canonical case introducing the
/// contract, so primary-less advisories stay deterministic in manifest order rather than
/// lexical contract order.
struct ContractFamily {
    contract: String,
    first_case_id: String,
    first_case_order: usize,
    roles: Vec<Option<CaseRole>>,
}

pub(crate) fn evaluate_suite(suite: &TestSuiteSpec) -> PolicyEvaluation {
    let case_groups = group_cases(&suite.cases);
    let mut evaluation = PolicyEvaluation::default();
    let mut primary_contract_owners = BTreeMap::<String, String>::new();

    for group in &case_groups {
        let first_case = &suite.cases[group.case_orders[0]];
        let role = first_case.role;
        let contract = first_case.contract.as_deref();

        // A missing role is a hard finding: every canonical case must declare ownership.
        if role.is_none() {
            evaluation.hard_findings.push(PolicyFinding::new(
                "missing_role_classification",
                Some(group.case_id.clone()),
                "Case has no manifest role classification.",
                FindingSortKey {
                    case_order: group.first_case_order,
                    backend_order: usize::MAX,
                    rule_order: RULE_MISSING_ROLE,
                },
            ));
        }

        // A missing contract is a hard finding for every non-smoke case. A contractless
        // smoke case is intentional (whole-case acceptance-only smoke), so it is exempt.
        // The primary-without-contract rule stays the single finding for primary cases so
        // one case never produces two missing-contract findings.
        if contract.is_none() && role != Some(CaseRole::Smoke) {
            if role == Some(CaseRole::Primary) {
                evaluation.hard_findings.push(PolicyFinding::new(
                    "primary_missing_contract",
                    Some(group.case_id.clone()),
                    "Primary case has no manifest contract classification.",
                    FindingSortKey {
                        case_order: group.first_case_order,
                        backend_order: usize::MAX,
                        rule_order: RULE_MISSING_CONTRACT,
                    },
                ));
            } else {
                evaluation.hard_findings.push(PolicyFinding::new(
                    "missing_contract_classification",
                    Some(group.case_id.clone()),
                    "Case has no manifest contract classification.",
                    FindingSortKey {
                        case_order: group.first_case_order,
                        backend_order: usize::MAX,
                        rule_order: RULE_MISSING_CONTRACT,
                    },
                ));
            }
        }

        // A primary case registers its contract; a second primary for the same contract is
        // a hard duplicate-primary finding attached to the later canonical case.
        if role == Some(CaseRole::Primary)
            && let Some(contract) = contract
            && let Some(previous_case_id) =
                primary_contract_owners.insert(contract.to_owned(), group.case_id.clone())
        {
            evaluation.hard_findings.push(PolicyFinding::new(
                "duplicate_primary_contract",
                Some(group.case_id.clone()),
                format!(
                    "Primary contract '{contract}' is also owned by case '{previous_case_id}'."
                ),
                FindingSortKey {
                    case_order: group.first_case_order,
                    backend_order: usize::MAX,
                    rule_order: RULE_DUPLICATE_PRIMARY,
                },
            ));
        }

        // A whole-case acceptance-only case must be smoke; a stronger contract on any
        // backend exempts the case from this rule.
        if is_whole_case_acceptance_only(suite, group) && role != Some(CaseRole::Smoke) {
            evaluation.hard_findings.push(PolicyFinding::new(
                "acceptance_only_requires_smoke_role",
                Some(group.case_id.clone()),
                "Whole-case acceptance-only cases must declare role = \"smoke\".",
                FindingSortKey {
                    case_order: group.first_case_order,
                    backend_order: usize::MAX,
                    rule_order: RULE_ACCEPTANCE_ONLY_SMOKE,
                },
            ));
        }

        // Per-backend contains-mode diagnostics require an authored non-blank reason.
        for (backend_order, case_order) in group.case_orders.iter().copied().enumerate() {
            let case = &suite.cases[case_order];
            let ExpectedOutcome::Failure(expectation) = &case.expected else {
                continue;
            };

            if expectation.diagnostic_match != DiagnosticMatchMode::Contains
                || expectation
                    .diagnostic_match_reason
                    .as_deref()
                    .is_some_and(|reason| !reason.trim().is_empty())
            {
                continue;
            }

            evaluation.hard_findings.push(PolicyFinding::new(
                "diagnostic_contains_requires_reason",
                Some(case.case_id.clone()),
                format!(
                    "Case '{}' backend '{}' uses diagnostic_match = \"contains\" without a non-blank authored diagnostic_match_reason.",
                    case.case_id,
                    case.backend_id.as_str()
                ),
                FindingSortKey {
                    case_order: group.first_case_order,
                    backend_order,
                    rule_order: RULE_CONTAINS_REASON,
                },
            ));
        }
    }

    emit_primary_less_contract_advisories(suite, &case_groups, &mut evaluation);

    evaluation
        .hard_findings
        .sort_by_key(|finding| finding.sort_key);
    evaluation
        .advisories
        .sort_by_key(|finding| finding.sort_key);

    evaluation
}

/// Reports one advisory per contract family that has no primary owner, emitted after the
/// complete suite is grouped. Backend-only and adversarial-only ownership are distinguished
/// with stable finding codes so they are reviewable rather than misreported as ordinary orphan
/// boundaries; any mixed/ordinary primary-less family stays visibly distinct.
fn emit_primary_less_contract_advisories(
    suite: &TestSuiteSpec,
    case_groups: &[CaseGroup],
    evaluation: &mut PolicyEvaluation,
) {
    let mut families: Vec<ContractFamily> = Vec::new();
    let mut family_index = BTreeMap::<String, usize>::new();

    for group in case_groups {
        let first_case = &suite.cases[group.case_orders[0]];
        let Some(contract) = first_case.contract.as_deref() else {
            continue;
        };

        if let Some(&index) = family_index.get(contract) {
            families[index].roles.push(first_case.role);
            continue;
        }

        family_index.insert(contract.to_owned(), families.len());
        families.push(ContractFamily {
            contract: contract.to_owned(),
            first_case_id: group.case_id.clone(),
            first_case_order: group.first_case_order,
            roles: vec![first_case.role],
        });
    }

    for family in &families {
        if family.roles.contains(&Some(CaseRole::Primary)) {
            continue;
        }

        let (code, ownership) = classify_primary_less_family(&family.roles);
        evaluation.advisories.push(PolicyFinding::new(
            code,
            Some(family.first_case_id.clone()),
            format!(
                "Contract '{}' has no primary owner; ownership is {}.",
                family.contract, ownership
            ),
            FindingSortKey {
                case_order: family.first_case_order,
                backend_order: usize::MAX,
                rule_order: RULE_PRIMARY_LESS_FAMILY,
            },
        ));
    }
}

fn classify_primary_less_family(roles: &[Option<CaseRole>]) -> (&'static str, &'static str) {
    if roles.iter().any(Option::is_none) {
        return (
            "primary_less_contract_unclassified",
            "unclassified (a member has no role)",
        );
    }
    if roles.iter().all(|role| *role == Some(CaseRole::Backend)) {
        return ("primary_less_contract_backend_only", "backend-only");
    }
    if roles
        .iter()
        .all(|role| *role == Some(CaseRole::Adversarial))
    {
        return ("primary_less_contract_adversarial_only", "adversarial-only");
    }
    (
        "primary_less_contract_mixed",
        "mixed or ordinary secondary ownership",
    )
}

pub(crate) fn format_hard_findings(evaluation: &PolicyEvaluation) -> String {
    let findings = evaluation
        .hard_findings
        .iter()
        .map(|finding| {
            let case_id = finding.case_id.as_deref().unwrap_or("<suite>");
            format!("{} ({case_id})", finding.code)
        })
        .collect::<Vec<_>>()
        .join(", ");

    format!("Integration suite policy rejected the loaded suite: {findings}.")
}

fn group_cases(cases: &[super::TestCaseSpec]) -> Vec<CaseGroup> {
    let mut groups: Vec<CaseGroup> = Vec::new();
    let mut group_indexes = BTreeMap::<String, usize>::new();

    for (case_order, case) in cases.iter().enumerate() {
        if let Some(group_index) = group_indexes.get(&case.case_id).copied() {
            groups[group_index].case_orders.push(case_order);
            continue;
        }

        let group_index = groups.len();
        group_indexes.insert(case.case_id.clone(), group_index);
        groups.push(CaseGroup {
            case_id: case.case_id.clone(),
            first_case_order: case_order,
            case_orders: vec![case_order],
        });
    }

    groups
}

fn is_whole_case_acceptance_only(suite: &TestSuiteSpec, group: &CaseGroup) -> bool {
    !group.case_orders.is_empty()
        && group.case_orders.iter().all(|case_order| {
            matches!(
                &suite.cases[*case_order].expected,
                ExpectedOutcome::Success(expectation)
                    if expectation.success_contract == Some(SuccessContract::AcceptanceOnly)
            )
        })
}

impl PolicyFinding {
    fn new(
        code: &str,
        case_id: Option<String>,
        message: impl Into<String>,
        sort_key: FindingSortKey,
    ) -> Self {
        Self {
            code: code.to_owned(),
            case_id,
            message: message.into(),
            sort_key,
        }
    }
}

const RULE_MISSING_ROLE: usize = 10;
const RULE_MISSING_CONTRACT: usize = 20;
const RULE_DUPLICATE_PRIMARY: usize = 30;
const RULE_ACCEPTANCE_ONLY_SMOKE: usize = 40;
const RULE_CONTAINS_REASON: usize = 50;
const RULE_PRIMARY_LESS_FAMILY: usize = 10;
