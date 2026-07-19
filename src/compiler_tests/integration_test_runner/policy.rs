//! Cross-case policy evaluation for the integration test suite.
//!
//! WHAT: evaluates ownership and assertion-strength rules after typed fixture loading.
//! WHY: cross-case policy must be decided once before reporting, selection or execution.

use super::{CaseRole, ExpectedOutcome, SuccessContract, TestSuiteSpec};
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

pub(crate) fn evaluate_suite(suite: &TestSuiteSpec) -> PolicyEvaluation {
    let case_groups = group_cases(&suite.cases);
    let mut evaluation = PolicyEvaluation::default();
    let mut primary_contract_owners = BTreeMap::<String, String>::new();

    for group in case_groups {
        let first_case = &suite.cases[group.case_orders[0]];

        if first_case.contract.is_none() && first_case.role != Some(CaseRole::Primary) {
            evaluation.advisories.push(PolicyFinding::new(
                "missing_contract_classification",
                Some(group.case_id.clone()),
                "Case has no manifest contract classification.",
                FindingSortKey {
                    case_order: group.first_case_order,
                    backend_order: usize::MAX,
                    rule_order: 10,
                },
            ));
        }

        if first_case.role.is_none() {
            evaluation.advisories.push(PolicyFinding::new(
                "missing_role_classification",
                Some(group.case_id.clone()),
                "Case has no manifest role classification.",
                FindingSortKey {
                    case_order: group.first_case_order,
                    backend_order: usize::MAX,
                    rule_order: 20,
                },
            ));
        }

        if first_case.role == Some(CaseRole::Primary) {
            if let Some(contract) = first_case.contract.as_ref() {
                if let Some(previous_case_id) =
                    primary_contract_owners.insert(contract.clone(), group.case_id.clone())
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
                            rule_order: 10,
                        },
                    ));
                }
            } else {
                evaluation.hard_findings.push(PolicyFinding::new(
                    "primary_missing_contract",
                    Some(group.case_id.clone()),
                    "Primary case has no manifest contract classification.",
                    FindingSortKey {
                        case_order: group.first_case_order,
                        backend_order: usize::MAX,
                        rule_order: 20,
                    },
                ));
            }
        }

        if is_whole_case_acceptance_only(suite, &group) && first_case.role != Some(CaseRole::Smoke)
        {
            evaluation.hard_findings.push(PolicyFinding::new(
                "acceptance_only_requires_smoke_role",
                Some(group.case_id.clone()),
                "Whole-case acceptance-only cases must declare role = \"smoke\".",
                FindingSortKey {
                    case_order: group.first_case_order,
                    backend_order: usize::MAX,
                    rule_order: 30,
                },
            ));
        }
    }

    evaluation
        .hard_findings
        .sort_by_key(|finding| finding.sort_key);
    evaluation
        .advisories
        .sort_by_key(|finding| finding.sort_key);

    evaluation
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
