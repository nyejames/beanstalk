//! Unit tests for `FrontendArenaCapacityEstimate` caps and monotonic behavior.

use crate::compiler_frontend::arena::{FrontendArenaCapacityEstimate, HeaderStats, TokenStats};

#[test]
fn empty_estimate_is_non_zero_for_scope_frames() {
    let capacity = FrontendArenaCapacityEstimate::new(
        1,
        0,
        TokenStats::default(),
        HeaderStats::default(),
        0,
        0,
    );

    assert!(capacity.scope_frames >= 1);
    assert_eq!(capacity.capped_field_count, 0);
}

#[test]
fn estimate_grows_with_token_count() {
    let small_stats = TokenStats {
        total_tokens: 100,
        ..Default::default()
    };

    let large_stats = TokenStats {
        total_tokens: 1000,
        ..Default::default()
    };

    let small =
        FrontendArenaCapacityEstimate::new(1, 100, small_stats, HeaderStats::default(), 0, 0);
    let large =
        FrontendArenaCapacityEstimate::new(1, 1000, large_stats, HeaderStats::default(), 0, 0);

    assert!(large.scope_frames >= small.scope_frames);
    assert!(large.expressions >= small.expressions);
    assert!(large.statements >= small.statements);
}

#[test]
fn estimate_grows_with_declarations() {
    let headers = HeaderStats {
        functions: 10,
        ..Default::default()
    };

    let capacity = FrontendArenaCapacityEstimate::new(1, 100, TokenStats::default(), headers, 0, 0);

    assert!(capacity.declarations >= 10);
    assert!(capacity.scope_frames >= 10);
}

#[test]
fn scope_estimate_grows_with_control_flow_tokens() {
    let flat_stats = TokenStats {
        total_tokens: 400,
        ..Default::default()
    };
    let nested_stats = TokenStats {
        total_tokens: 400,
        if_tokens: 20,
        loop_tokens: 10,
        catch_tokens: 8,
        then_tokens: 24,
        template_markers: 30,
        ..Default::default()
    };

    let flat =
        FrontendArenaCapacityEstimate::new(1, 1000, flat_stats, HeaderStats::default(), 0, 0);
    let nested =
        FrontendArenaCapacityEstimate::new(1, 1000, nested_stats, HeaderStats::default(), 0, 0);

    assert!(
        nested.scope_frames > flat.scope_frames,
        "explicit block/template syntax should raise the scope-frame estimate"
    );
}

#[test]
fn scope_estimate_grows_with_header_structure() {
    let shallow_headers = HeaderStats {
        functions: 4,
        ..Default::default()
    };
    let structured_headers = HeaderStats {
        functions: 4,
        const_templates: 3,
        signature_members: 64,
        choice_variants: 16,
        generic_parameters: 8,
        ..Default::default()
    };

    let shallow =
        FrontendArenaCapacityEstimate::new(1, 1000, TokenStats::default(), shallow_headers, 0, 0);
    let structured = FrontendArenaCapacityEstimate::new(
        1,
        1000,
        TokenStats::default(),
        structured_headers,
        0,
        0,
    );

    assert!(
        structured.scope_frames > shallow.scope_frames,
        "signature, generic, variant, and const-template structure should contribute to scope pressure"
    );
}

#[test]
fn estimate_grows_with_source_byte_count() {
    let small = FrontendArenaCapacityEstimate::new(
        1,
        0,
        TokenStats::default(),
        HeaderStats::default(),
        0,
        0,
    );
    let large = FrontendArenaCapacityEstimate::new(
        1,
        16 * 1024,
        TokenStats::default(),
        HeaderStats::default(),
        0,
        0,
    );

    assert!(large.scope_frames >= small.scope_frames);
    assert!(large.expressions > small.expressions);
}

#[test]
fn hard_cap_limits_huge_inputs() {
    let stats = TokenStats {
        total_tokens: usize::MAX,
        ..Default::default()
    };

    let capacity = FrontendArenaCapacityEstimate::new(
        usize::MAX,
        usize::MAX,
        stats,
        HeaderStats::default(),
        usize::MAX,
        usize::MAX,
    );

    assert_eq!(capacity.scope_frames, 1_000_000);
    assert!(capacity.capped_field_count > 0);
}

#[test]
fn templates_scale_with_fragments() {
    let headers = HeaderStats {
        const_templates: 2,
        ..Default::default()
    };

    let capacity = FrontendArenaCapacityEstimate::new(1, 100, TokenStats::default(), headers, 0, 3);

    assert!(capacity.templates >= 5);
    assert!(capacity.template_atoms >= capacity.templates);
    assert!(capacity.render_pieces >= capacity.templates);
}
