# Type Environment Redesign Plan

## Summary

Deferred follow-up for the AST pipeline restructure. This plan should start only after the AST pipeline refactor has removed the current build-state, declaration-table, constant-resolution, scope-context, and parser churn bottlenecks.

## Goals

- Introduce compact type IDs for common frontend type identity.
- Move nominal type definitions into a table instead of repeatedly carrying large `DataType` payloads.
- Intern generic nominal instances so repeated instantiations share one canonical representation.
- Reduce avoidable `DataType` cloning across AST, HIR preparation, and diagnostics.
- Separate type identity from layout/backend representation so later lowering stages can make target-specific decisions without bloating frontend type checks.

## Non-Goals

- Do not change Beanstalk language semantics as part of the redesign.
- Do not mix this with the current AST pipeline restructure phases.
- Do not preserve compatibility wrappers for old type APIs; Beanstalk is pre-release.

## Initial Acceptance Criteria

- Type identity lookup is table-backed and deterministic.
- Generic nominal instantiations are interned and reusable.
- Diagnostics still render source-level type names clearly through the shared `StringTable`.
- AST and HIR boundaries remain explicit about which stage owns type resolution and which stage owns lowering.
