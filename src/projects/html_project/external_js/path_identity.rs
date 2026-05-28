//! Stable path-derived identifiers for HTML external JavaScript assets.
//!
//! WHAT: centralizes filename/package-safe stems and deterministic path hashes.
//! WHY: provider package identities and emitted asset filenames both need stable
//! path-derived names without relying on process-random hashers or duplicating
//! sanitization rules.

use std::path::Path;

/// Returns a sanitized stem suitable for generated package IDs and output filenames.
pub(crate) fn sanitized_path_stem(path: &Path, fallback: &str) -> String {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(fallback);
    sanitize_identifier_component(stem, fallback)
}

/// Computes a stable 64-bit FNV-1a hash for a canonical path and returns lowercase hex.
pub(crate) fn stable_path_hash_hex(path: &Path) -> String {
    stable_hash_hex(&path.display().to_string())
}

fn sanitize_identifier_component(value: &str, fallback: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();

    if sanitized.is_empty() {
        fallback.to_owned()
    } else {
        sanitized
    }
}

fn stable_hash_hex(input: &str) -> String {
    const FNV_OFFSET_BASIS: u64 = 14695981039346656037;
    const FNV_PRIME: u64 = 1099511628211;

    let mut hash = FNV_OFFSET_BASIS;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    format!("{hash:016x}")
}
