//! NPAP v1 hashing utilities for proc-macro code generation.
//!
//! This module provides compile-time hashing functions for generating
//! binding IDs and impl hashes per GOLD_PLAN.md specifications.

use unicode_normalization::UnicodeNormalization;

/// Normalizes a string per GOLD_PLAN §7.0.2:
/// 1. Unicode NFC normalization
/// 2. Newline normalization (`\r\n` and `\r` → `\n`)
pub(crate) fn normalize_string(s: &str) -> String {
    let nfc: String = s.nfc().collect();
    normalize_newlines(&nfc)
}

/// Normalizes newlines: `\r\n` → `\n`, standalone `\r` → `\n`
fn normalize_newlines(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            result.push('\n');
        } else {
            result.push(c);
        }
    }

    result
}

/// Computes BLAKE3-256 hash and returns lowercase hex string (64 chars).
pub(crate) fn blake3_256_lowerhex(data: &[u8]) -> String {
    let hash = blake3::hash(data);
    hash.to_hex().to_string()
}

/// Generates a binding ID from kind and expression per GOLD_PLAN §4.2.1.
///
/// Formula: `blake3_256_lowerhex("namako-binding-id-v1|" + kind + "|" + expr_norm)`
pub(crate) fn generate_binding_id(kind: &str, expression: &str) -> String {
    let expr_norm = normalize_string(expression);
    let input = format!("namako-binding-id-v1|{kind}|{expr_norm}");
    blake3_256_lowerhex(input.as_bytes())
}

/// Generates an impl_hash from a function body token stream per GOLD_PLAN §6.2.2.
///
/// The token-fingerprint-v1 scheme:
/// 1. Token stream of function body (excluding signature and attributes)
/// 2. UTF-8 encoding, NFC normalization, newlines normalized to `\n`
/// 3. Whitespace collapsed to single spaces between tokens
/// 4. Comments excluded (already done by syn parsing)
/// 5. BLAKE3-256 hash
pub(crate) fn generate_impl_hash(func_body: &syn::Block) -> String {
    // Convert block to token stream, then to normalized string
    let tokens = quote::quote!(#func_body);
    let token_str = tokens.to_string();

    // Normalize: NFC + newlines
    let normalized = normalize_string(&token_str);

    // Collapse whitespace (tokens are already space-separated by quote)
    let collapsed = collapse_whitespace(&normalized);

    blake3_256_lowerhex(collapsed.as_bytes())
}

/// Collapses multiple whitespace characters to single spaces.
fn collapse_whitespace(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut last_was_whitespace = false;

    for c in s.chars() {
        if c.is_whitespace() {
            if !last_was_whitespace {
                result.push(' ');
                last_was_whitespace = true;
            }
        } else {
            result.push(c);
            last_was_whitespace = false;
        }
    }

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binding_id_format() {
        let id = generate_binding_id("Given", "a server is running");
        assert_eq!(id.len(), 64);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_binding_id_deterministic() {
        let id1 = generate_binding_id("When", "a client connects");
        let id2 = generate_binding_id("When", "a client connects");
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_binding_id_different_kinds() {
        let given = generate_binding_id("Given", "something");
        let when = generate_binding_id("When", "something");
        assert_ne!(given, when);
    }

    #[test]
    fn test_normalize_crlf() {
        assert_eq!(normalize_string("a\r\nb"), "a\nb");
        assert_eq!(normalize_string("a\rb"), "a\nb");
    }

    #[test]
    fn test_collapse_whitespace() {
        assert_eq!(collapse_whitespace("a   b  c"), "a b c");
        assert_eq!(collapse_whitespace("  a  \n\n  b  "), "a b");
    }

    #[test]
    fn test_golden_binding_id() {
        // Must match the value in namako/src/npap.rs
        let id = generate_binding_id("Given", "a server is running");
        assert_eq!(
            id,
            "479972f440a609dcdd70639c3820df553b4b7d0a47b563234a5bab31b4089f6d"
        );
    }
}
