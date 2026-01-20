//! NPAP v1 Canonical Encoding and Hashing Module
//!
//! This module implements the **single source of truth** for all hashing and
//! canonical encoding in Namako v1, per GOLD_PLAN.md §7.0.
//!
//! # Hash Contract Version
//!
//! v1 uses: `"namako-v1-json+blake3-256"`
//!
//! # Core Guarantees
//!
//! 1. **String Normalization**: UTF-8, NFC Unicode, `\n` line endings
//! 2. **Canonical JSON**: Sorted keys, no floats, explicit nulls for optionals
//! 3. **BLAKE3-256**: Lowercase hex output (64 chars)

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

/// The v1 hash contract version identifier.
pub const HASH_CONTRACT_VERSION: &str = "namako-v1-json+blake3-256";

/// The v1 NPAP protocol version.
pub const NPAP_VERSION: u32 = 1;

/// The v1 binding ID scheme identifier.
pub const BINDING_ID_SCHEME: &str = "kind+expr_norm|namako-binding-id-v1|blake3-256-lowerhex";

/// The v1 impl_hash scheme identifier.
pub const IMPL_HASH_SCHEME: &str = "token-fingerprint-v1|blake3-256-lowerhex";

// ============================================================================
// String Normalization (§7.0.2)
// ============================================================================

/// Normalizes a string per GOLD_PLAN §7.0.2:
/// 1. Unicode NFC normalization
/// 2. Newline normalization (`\r\n` and `\r` → `\n`)
///
/// # Examples
///
/// ```
/// use namako::npap::normalize_string;
///
/// assert_eq!(normalize_string("hello\r\nworld"), "hello\nworld");
/// assert_eq!(normalize_string("café"), "café"); // NFC normalized
/// ```
#[must_use]
pub fn normalize_string(s: &str) -> String {
    // First apply NFC normalization
    let nfc: String = s.nfc().collect();

    // Then normalize line endings: \r\n → \n, standalone \r → \n
    normalize_newlines(&nfc)
}

/// Normalizes newlines: `\r\n` → `\n`, standalone `\r` → `\n`
#[must_use]
fn normalize_newlines(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\r' {
            // Convert \r or \r\n to \n
            if chars.peek() == Some(&'\n') {
                chars.next(); // consume the \n
            }
            result.push('\n');
        } else {
            result.push(c);
        }
    }

    result
}

// ============================================================================
// Canonical JSON Encoding (§7.0.3)
// ============================================================================

/// Encodes a serializable value to canonical JSON per GOLD_PLAN §7.0.3:
/// - Object keys sorted lexicographically
/// - No trailing commas, no comments
/// - Integers only (no floats)
/// - Optional fields encoded as explicit `null`
///
/// # Errors
///
/// Returns an error if serialization fails (e.g., contains floats).
///
/// # Examples
///
/// ```
/// use namako::npap::canonical_json_encode;
/// use serde_json::json;
///
/// let value = json!({"z": 1, "a": 2, "m": null});
/// let encoded = canonical_json_encode(&value).unwrap();
/// assert_eq!(encoded, r#"{"a":2,"m":null,"z":1}"#);
/// ```
pub fn canonical_json_encode<T: Serialize>(value: &T) -> Result<String, CanonicalJsonError> {
    // First serialize to serde_json::Value to enable sorting
    let json_value = serde_json::to_value(value)
        .map_err(|e| CanonicalJsonError::SerializationFailed(e.to_string()))?;

    // Validate and sort the value
    let canonical = canonicalize_value(json_value)?;

    // Serialize to compact JSON string
    serde_json::to_string(&canonical)
        .map_err(|e| CanonicalJsonError::SerializationFailed(e.to_string()))
}

/// Errors that can occur during canonical JSON encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalJsonError {
    /// Floats are forbidden in hashed objects (§7.0.1)
    FloatForbidden,
    /// Serialization failed
    SerializationFailed(String),
}

impl std::fmt::Display for CanonicalJsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FloatForbidden => {
                write!(f, "floats are forbidden in hashed objects per NPAP v1")
            }
            Self::SerializationFailed(msg) => {
                write!(f, "canonical JSON serialization failed: {msg}")
            }
        }
    }
}

impl std::error::Error for CanonicalJsonError {}

/// Recursively canonicalizes a JSON value:
/// - Sorts object keys
/// - Validates no floats are present
fn canonicalize_value(value: Value) -> Result<Value, CanonicalJsonError> {
    match value {
        Value::Object(map) => {
            // Sort keys and recursively canonicalize values
            let mut sorted: BTreeMap<String, Value> = BTreeMap::new();
            for (k, v) in map {
                sorted.insert(k, canonicalize_value(v)?);
            }
            Ok(Value::Object(sorted.into_iter().collect()))
        }
        Value::Array(arr) => {
            // Preserve array order, recursively canonicalize elements
            let canonical: Result<Vec<Value>, _> =
                arr.into_iter().map(canonicalize_value).collect();
            Ok(Value::Array(canonical?))
        }
        Value::Number(n) => {
            // Reject floats (§7.0.1)
            if n.is_f64() && !n.is_i64() && !n.is_u64() {
                // It's a float that can't be represented as integer
                return Err(CanonicalJsonError::FloatForbidden);
            }
            Ok(Value::Number(n))
        }
        // Null, Bool, String pass through unchanged
        other => Ok(other),
    }
}

// ============================================================================
// BLAKE3-256 Hashing (§7.0.6)
// ============================================================================

/// Computes BLAKE3-256 hash and returns lowercase hex string (64 chars).
///
/// This is the authoritative hash function for all NPAP v1 operations.
///
/// # Examples
///
/// ```
/// use namako::npap::blake3_256_lowerhex;
///
/// let hash = blake3_256_lowerhex(b"hello");
/// assert_eq!(hash.len(), 64);
/// assert!(hash.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
/// ```
#[must_use]
pub fn blake3_256_lowerhex(data: &[u8]) -> String {
    let hash = blake3::hash(data);
    hash.to_hex().to_string()
}

/// Computes BLAKE3-256 hash of a string after normalization.
///
/// Convenience function that normalizes the string first, then hashes.
#[must_use]
pub fn blake3_256_lowerhex_normalized(s: &str) -> String {
    let normalized = normalize_string(s);
    blake3_256_lowerhex(normalized.as_bytes())
}

// ============================================================================
// Binding ID Generation (§4.2.1)
// ============================================================================

/// Generates a binding ID from kind and expression per GOLD_PLAN §4.2.1.
///
/// Formula: `blake3_256_lowerhex("namako-binding-id-v1|" + kind + "|" + expr_norm)`
///
/// # Arguments
///
/// * `kind` - The step kind: "Given", "When", or "Then"
/// * `expression` - The cucumber expression string
///
/// # Examples
///
/// ```
/// use namako::npap::generate_binding_id;
///
/// let id = generate_binding_id("Given", "a server is running");
/// assert_eq!(id.len(), 64);
/// ```
#[must_use]
pub fn generate_binding_id(kind: &str, expression: &str) -> String {
    let expr_norm = normalize_string(expression);
    let input = format!("namako-binding-id-v1|{kind}|{expr_norm}");
    blake3_256_lowerhex(input.as_bytes())
}

// ============================================================================
// Feature Fingerprint (§7.3.1)
// ============================================================================

/// Computes the feature fingerprint hash for a collection of feature files.
///
/// Per GOLD_PLAN §7.3.1:
/// - Files sorted by relative path (lexicographic)
/// - Each file's content normalized (NFC, newlines)
/// - Result hashed with BLAKE3-256
///
/// # Arguments
///
/// * `files` - Iterator of (relative_path, content) pairs
///
/// # Examples
///
/// ```
/// use namako::npap::compute_feature_fingerprint;
///
/// let files = vec![
///     ("specs/features/a.feature", "Feature: A\n"),
///     ("specs/features/b.feature", "Feature: B\n"),
/// ];
/// let hash = compute_feature_fingerprint(files.into_iter());
/// assert_eq!(hash.len(), 64);
/// ```
#[must_use]
pub fn compute_feature_fingerprint<'a, I>(files: I) -> String
where
    I: Iterator<Item = (&'a str, &'a str)>,
{
    // Collect and sort by path
    let mut entries: Vec<_> = files.collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));

    // Build the fingerprint structure
    let fingerprint: Vec<FileFingerprint> = entries
        .into_iter()
        .map(|(path, content)| {
            let normalized_content = normalize_string(content);
            let content_hash = blake3_256_lowerhex(normalized_content.as_bytes());
            FileFingerprint {
                path: path.to_string(),
                content_hash,
            }
        })
        .collect();

    // Serialize to canonical JSON and hash
    let json = canonical_json_encode(&fingerprint)
        .expect("file fingerprint should always serialize");
    blake3_256_lowerhex(json.as_bytes())
}

/// Internal structure for feature fingerprint computation.
#[derive(Serialize)]
struct FileFingerprint {
    path: String,
    content_hash: String,
}

// ============================================================================
// Scenario Key Derivation (§6.4.3)
// ============================================================================

/// Derives a scenario key per GOLD_PLAN §6.4.3.
///
/// Format: `normalized_relpath:L<line_number>`
///
/// # Arguments
///
/// * `relative_path` - Path from repo root to feature file
/// * `line_number` - 1-based line number where Scenario keyword appears
///
/// # Examples
///
/// ```
/// use namako::npap::derive_scenario_key;
///
/// let key = derive_scenario_key("specs/features/smoke/test.feature", 10);
/// assert_eq!(key, "specs/features/smoke/test.feature:L10");
/// ```
///
/// # Deprecated (v1.5)
///
/// Use `id_tags::derive_scenario_key_from_ids()` instead. Line-based keys are fragile under
/// refactoring and don't survive scenario reordering or file reorganization.
#[deprecated(
    since = "1.5",
    note = "Use id_tags::derive_scenario_key_from_ids instead. Line-based keys are fragile under refactoring."
)]
#[must_use]
pub fn derive_scenario_key(relative_path: &str, line_number: u32) -> String {
    let normalized_path = normalize_path(relative_path);
    format!("{normalized_path}:L{line_number}")
}

/// Derives a scenario outline example key per GOLD_PLAN §6.4.3.
///
/// Format: `normalized_relpath:L<scenario_line>:E<examples_idx>:R<row_idx>`
///
/// # Arguments
///
/// * `relative_path` - Path from repo root to feature file
/// * `scenario_line` - 1-based line number where Scenario Outline keyword appears
/// * `examples_block_idx` - 0-based index of Examples block
/// * `row_idx` - 0-based index of data row within Examples block
///
/// # Deprecated (v1.5)
///
/// Use `id_tags::derive_scenario_outline_key_from_ids()` instead. Line-based keys are fragile
/// under refactoring and don't survive scenario reordering or file reorganization.
#[deprecated(
    since = "1.5",
    note = "Use id_tags::derive_scenario_outline_key_from_ids instead. Line-based keys are fragile under refactoring."
)]
#[must_use]
pub fn derive_scenario_outline_key(
    relative_path: &str,
    scenario_line: u32,
    examples_block_idx: u32,
    row_idx: u32,
) -> String {
    let normalized_path = normalize_path(relative_path);
    format!("{normalized_path}:L{scenario_line}:E{examples_block_idx}:R{row_idx}")
}

/// Normalizes a file path per GOLD_PLAN §6.4.3:
/// - Forward slashes only
/// - NFC Unicode normalization
/// - No leading "./" or trailing "/"
#[must_use]
fn normalize_path(path: &str) -> String {
    let mut normalized: String = path.nfc().collect();

    // Convert backslashes to forward slashes
    normalized = normalized.replace('\\', "/");

    // Remove leading "./"
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }

    // Remove trailing "/"
    while normalized.ends_with('/') {
        normalized.pop();
    }

    normalized
}

// ============================================================================
// NPAP v1 Type Definitions (per ARCH_LOCK.md §4)
// ============================================================================

use serde::Deserialize;

/// Semantic Step Registry returned by adapter manifest command.
///
/// Per GOLD_PLAN §6.2.1, this is the authoritative list of bindings
/// available in the adapter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticStepRegistry {
    /// Protocol version (v1 = 1)
    pub npap_version: u32,
    /// Hash contract identifier
    pub hash_contract_version: String,
    /// Binding ID derivation scheme
    pub binding_id_scheme: String,
    /// Impl hash derivation scheme
    pub impl_hash_scheme: String,
    /// Hash of the registry (excluding this field)
    pub step_registry_hash: String,
    /// All registered step bindings, sorted by binding_id
    pub bindings: Vec<SemanticBinding>,
}

impl SemanticStepRegistry {
    /// Creates a new registry and computes its hash.
    ///
    /// The `bindings` will be sorted by `binding_id` before hashing.
    pub fn new(mut bindings: Vec<SemanticBinding>) -> Self {
        // Sort bindings by binding_id for deterministic ordering
        bindings.sort_by(|a, b| a.binding_id.cmp(&b.binding_id));

        // Build a temporary struct for hashing (without step_registry_hash)
        let for_hashing = RegistryForHashing {
            npap_version: NPAP_VERSION,
            hash_contract_version: HASH_CONTRACT_VERSION.to_string(),
            binding_id_scheme: BINDING_ID_SCHEME.to_string(),
            impl_hash_scheme: IMPL_HASH_SCHEME.to_string(),
            bindings: &bindings,
        };

        let json = canonical_json_encode(&for_hashing)
            .expect("registry should serialize");
        let step_registry_hash = blake3_256_lowerhex(json.as_bytes());

        Self {
            npap_version: NPAP_VERSION,
            hash_contract_version: HASH_CONTRACT_VERSION.to_string(),
            binding_id_scheme: BINDING_ID_SCHEME.to_string(),
            impl_hash_scheme: IMPL_HASH_SCHEME.to_string(),
            step_registry_hash,
            bindings,
        }
    }
}

/// Helper struct for computing registry hash (excludes step_registry_hash field).
#[derive(Serialize)]
struct RegistryForHashing<'a> {
    npap_version: u32,
    hash_contract_version: String,
    binding_id_scheme: String,
    impl_hash_scheme: String,
    bindings: &'a [SemanticBinding],
}

/// A single step binding in the semantic registry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticBinding {
    /// Binding ID computed from (kind, expression)
    pub binding_id: String,
    /// Step kind: "Given", "When", or "Then"
    pub kind: String,
    /// The cucumber expression string
    pub expression: String,
    /// Signature metadata
    pub signature: BindingSignature,
    /// Implementation hash for drift detection
    pub impl_hash: String,
}

/// Signature metadata for a binding per GOLD_PLAN §4.4.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BindingSignature {
    /// Number of capture parameters expected
    pub captures_arity: u32,
    /// Whether binding accepts a DocString
    pub accepts_docstring: bool,
    /// Whether binding accepts a DataTable
    pub accepts_datatable: bool,
}

/// Header section for resolved plan per GOLD_PLAN §6.4.1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedPlanHeader {
    /// Protocol version
    pub npap_version: u32,
    /// Hash contract identifier
    pub hash_contract_version: String,
    /// Hash of feature files
    pub feature_fingerprint_hash: String,
    /// Hash of step registry used for resolution
    pub step_registry_hash: String,
    /// Hash of this resolved plan
    pub resolved_plan_hash: String,
}

/// Resolved Execution Plan produced by `namako lint` per GOLD_PLAN §6.4.1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedPlan {
    /// Header containing version and hash fields
    pub header: ResolvedPlanHeader,
    /// Resolved scenarios sorted by scenario_key
    pub scenarios: Vec<ResolvedScenario>,
}

impl ResolvedPlan {
    /// Creates a new resolved plan and computes its hash.
    pub fn new(
        feature_fingerprint_hash: String,
        step_registry_hash: String,
        mut scenarios: Vec<ResolvedScenario>,
    ) -> Self {
        // Sort scenarios by scenario_key
        scenarios.sort_by(|a, b| a.scenario_key.cmp(&b.scenario_key));

        // Build temp struct for hashing (matches JSON structure)
        let for_hashing = PlanForHashing {
            header: PlanHeaderForHashing {
                npap_version: NPAP_VERSION,
                hash_contract_version: HASH_CONTRACT_VERSION.to_string(),
                feature_fingerprint_hash: &feature_fingerprint_hash,
                step_registry_hash: &step_registry_hash,
            },
            scenarios: &scenarios,
        };

        let json = canonical_json_encode(&for_hashing)
            .expect("plan should serialize");
        let resolved_plan_hash = blake3_256_lowerhex(json.as_bytes());

        let header = ResolvedPlanHeader {
            npap_version: NPAP_VERSION,
            hash_contract_version: HASH_CONTRACT_VERSION.to_string(),
            feature_fingerprint_hash,
            step_registry_hash,
            resolved_plan_hash,
        };

        Self { header, scenarios }
    }
}

/// Helper struct for computing plan hash header (excludes resolved_plan_hash).
#[derive(Serialize)]
struct PlanHeaderForHashing<'a> {
    npap_version: u32,
    hash_contract_version: String,
    feature_fingerprint_hash: &'a str,
    step_registry_hash: &'a str,
}

/// Helper struct for computing plan hash.
#[derive(Serialize)]
struct PlanForHashing<'a> {
    header: PlanHeaderForHashing<'a>,
    scenarios: &'a [ResolvedScenario],
}

/// A resolved scenario with all steps mapped to bindings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedScenario {
    /// Unique scenario identifier
    pub scenario_key: String,
    /// Path to feature file
    pub feature_path: String,
    /// Scenario name
    pub scenario_name: String,
    /// Resolved steps
    pub steps: Vec<PlannedStep>,
}

/// A planned step with binding and captured values per GOLD_PLAN §6.4.1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannedStep {
    /// Effective step kind: "Given", "When", "Then" (after And/But resolution)
    pub effective_kind: String,
    /// Original step text from AST
    pub step_text: String,
    /// Binding ID to dispatch
    pub binding_id: String,
    /// Captured values from expression matching
    pub captures: Vec<String>,
    /// DocString if present
    pub docstring: Option<String>,
    /// DataTable if present (rows of cells)
    pub datatable: Option<Vec<Vec<String>>>,
    /// Hash of execution payload per GOLD_PLAN §6.5
    pub payload_hash: String,
}

impl PlannedStep {
    /// Creates a new planned step and computes its payload hash per GOLD_PLAN §6.5.
    pub fn new(
        effective_kind: String,
        step_text: String,
        binding_id: String,
        captures: Vec<String>,
        docstring: Option<String>,
        datatable: Option<Vec<Vec<String>>>,
    ) -> Self {
        let payload = ExecutionPayload {
            effective_kind: &effective_kind,
            step_text: &step_text,
            binding_id: &binding_id,
            captures: &captures,
            docstring: docstring.as_deref(),
            datatable: datatable.as_ref(),
        };

        let json = canonical_json_encode(&payload)
            .expect("payload should serialize");
        let payload_hash = blake3_256_lowerhex(json.as_bytes());

        Self {
            effective_kind,
            step_text,
            binding_id,
            captures,
            docstring,
            datatable,
            payload_hash,
        }
    }
}

/// Execution payload structure for hashing per GOLD_PLAN §6.5.
/// All 6 fields are included in the hash for step identity.
#[derive(Serialize)]
struct ExecutionPayload<'a> {
    /// Effective kind after And/But resolution
    effective_kind: &'a str,
    /// Original step text from AST
    step_text: &'a str,
    /// Binding ID to dispatch
    binding_id: &'a str,
    /// Captured parameter values
    captures: &'a [String],
    /// DocString (explicit null if absent)
    docstring: Option<&'a str>,
    /// DataTable (explicit null if absent)
    datatable: Option<&'a Vec<Vec<String>>>,
}

/// Header section for run report per GOLD_PLAN §6.4.2.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunReportHeader {
    /// Protocol version
    pub npap_version: u32,
    /// Hash contract identifier
    pub hash_contract_version: String,
    /// Echoed from plan
    pub feature_fingerprint_hash: String,
    /// Echoed from plan
    pub step_registry_hash: String,
    /// Echoed from plan
    pub resolved_plan_hash: String,
    /// Hash of this run report
    pub run_report_hash: String,
}

/// Run Report produced by adapter after execution per GOLD_PLAN §6.4.2.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunReport {
    /// Header containing version and hash fields
    pub header: RunReportHeader,
    /// Scenario execution results sorted by scenario_key
    pub scenarios: Vec<ScenarioResult>,
}

impl RunReport {
    /// Creates a new run report and computes its hash.
    pub fn new(
        feature_fingerprint_hash: String,
        step_registry_hash: String,
        resolved_plan_hash: String,
        mut scenarios: Vec<ScenarioResult>,
    ) -> Self {
        // Sort scenarios by scenario_key
        scenarios.sort_by(|a, b| a.scenario_key.cmp(&b.scenario_key));

        let for_hashing = ReportForHashing {
            header: ReportHeaderForHashing {
                npap_version: NPAP_VERSION,
                hash_contract_version: HASH_CONTRACT_VERSION.to_string(),
                feature_fingerprint_hash: &feature_fingerprint_hash,
                step_registry_hash: &step_registry_hash,
                resolved_plan_hash: &resolved_plan_hash,
            },
            scenarios: &scenarios,
        };

        let json = canonical_json_encode(&for_hashing)
            .expect("report should serialize");
        let run_report_hash = blake3_256_lowerhex(json.as_bytes());

        let header = RunReportHeader {
            npap_version: NPAP_VERSION,
            hash_contract_version: HASH_CONTRACT_VERSION.to_string(),
            feature_fingerprint_hash,
            step_registry_hash,
            resolved_plan_hash,
            run_report_hash,
        };

        Self { header, scenarios }
    }
}

/// Helper struct for computing report hash header (excludes run_report_hash).
#[derive(Serialize)]
struct ReportHeaderForHashing<'a> {
    npap_version: u32,
    hash_contract_version: String,
    feature_fingerprint_hash: &'a str,
    step_registry_hash: &'a str,
    resolved_plan_hash: &'a str,
}

/// Helper struct for computing report hash.
#[derive(Serialize)]
struct ReportForHashing<'a> {
    header: ReportHeaderForHashing<'a>,
    scenarios: &'a [ScenarioResult],
}

/// Result of executing a single scenario.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScenarioResult {
    /// Scenario key
    pub scenario_key: String,
    /// Overall scenario status
    pub status: ScenarioStatus,
    /// Per-step results
    pub steps: Vec<StepResult>,
}

/// Scenario execution status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScenarioStatus {
    /// All steps passed
    Passed,
    /// At least one step failed
    Failed,
    /// Scenario was skipped
    Skipped,
}

/// Result of executing a single step per GOLD_PLAN §6.4.2.
/// Contains both planned and executed values for verify comparison per §7.4.2.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StepResult {
    /// Binding ID from the resolved plan
    pub planned_binding_id: String,
    /// Binding ID that was actually executed
    pub executed_binding_id: String,
    /// Payload hash from the resolved plan
    pub planned_payload_hash: String,
    /// Hash of actual execution payload
    pub executed_payload_hash: String,
    /// Impl hash at time of execution
    pub executed_impl_hash: String,
    /// Step execution status
    pub status: StepStatus,
    /// Error message if failed (optional extension)
    pub error_message: Option<String>,
}

/// Step execution status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    /// Step passed
    Passed,
    /// Step failed
    Failed,
    /// Step was skipped
    Skipped,
}

/// Certification artifact for baseline/candidate comparison.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Certification {
    /// Identity fields (used for strict equality check)
    pub identity: CertificationIdentity,
    /// Metadata fields (informational only)
    pub metadata: CertificationMetadata,
}

/// Identity fields that must match exactly for verification per GOLD_PLAN §7.3.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CertificationIdentity {
    /// Hash contract version (encoding + hashing rules)
    pub hash_contract_version: String,
    /// Hash of feature files
    pub feature_fingerprint_hash: String,
    /// Hash of step registry
    pub step_registry_hash: String,
    /// Hash of resolved plan
    pub resolved_plan_hash: String,
}

/// Informational metadata about certification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CertificationMetadata {
    /// When certification was created
    pub timestamp: String,
    /// Namako version that created it
    pub namako_version: String,
    /// NPAP protocol version
    pub npap_version: u32,
    /// Hash of run report (informational, not part of identity comparison)
    pub run_report_hash: String,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -------------------------------------------------------------------------
    // String Normalization Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_normalize_string_nfc() {
        // é can be represented as single char (U+00E9) or e + combining acute (U+0065 U+0301)
        // NFC should normalize to single char form
        let decomposed = "e\u{0301}"; // e + combining acute
        let composed = "é"; // precomposed

        assert_eq!(normalize_string(decomposed), composed);
        assert_eq!(normalize_string(composed), composed);
    }

    #[test]
    fn test_normalize_string_crlf() {
        assert_eq!(normalize_string("a\r\nb"), "a\nb");
        assert_eq!(normalize_string("a\rb"), "a\nb");
        assert_eq!(normalize_string("a\nb"), "a\nb");
        assert_eq!(normalize_string("a\r\n\r\nb"), "a\n\nb");
    }

    #[test]
    fn test_normalize_string_mixed() {
        // Both NFC and newline normalization
        let input = "café\r\ntest\re\u{0301}";
        let expected = "café\ntest\né";
        assert_eq!(normalize_string(input), expected);
    }

    // -------------------------------------------------------------------------
    // Canonical JSON Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_canonical_json_key_ordering() {
        let value = json!({"z": 1, "a": 2, "m": 3});
        let encoded = canonical_json_encode(&value).unwrap();
        assert_eq!(encoded, r#"{"a":2,"m":3,"z":1}"#);
    }

    #[test]
    fn test_canonical_json_nested_ordering() {
        let value = json!({
            "z": {"b": 1, "a": 2},
            "a": {"d": 3, "c": 4}
        });
        let encoded = canonical_json_encode(&value).unwrap();
        assert_eq!(encoded, r#"{"a":{"c":4,"d":3},"z":{"a":2,"b":1}}"#);
    }

    #[test]
    fn test_canonical_json_explicit_null() {
        let value = json!({"a": null, "b": 1});
        let encoded = canonical_json_encode(&value).unwrap();
        assert_eq!(encoded, r#"{"a":null,"b":1}"#);
    }

    #[test]
    fn test_canonical_json_array_preserves_order() {
        let value = json!([3, 1, 2]);
        let encoded = canonical_json_encode(&value).unwrap();
        assert_eq!(encoded, "[3,1,2]");
    }

    #[test]
    fn test_canonical_json_integers() {
        let value = json!({"int": 42, "neg": -10, "zero": 0});
        let encoded = canonical_json_encode(&value).unwrap();
        // Keys sorted: "int", "neg", "zero"
        assert_eq!(encoded, r#"{"int":42,"neg":-10,"zero":0}"#);
    }

    // -------------------------------------------------------------------------
    // BLAKE3 Hashing Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_blake3_output_format() {
        let hash = blake3_256_lowerhex(b"test");
        assert_eq!(hash.len(), 64, "hash should be 64 hex chars");
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(hash.chars().all(|c| !c.is_ascii_uppercase()));
    }

    #[test]
    fn test_blake3_deterministic() {
        let h1 = blake3_256_lowerhex(b"hello world");
        let h2 = blake3_256_lowerhex(b"hello world");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_blake3_different_inputs() {
        let h1 = blake3_256_lowerhex(b"hello");
        let h2 = blake3_256_lowerhex(b"world");
        assert_ne!(h1, h2);
    }

    // -------------------------------------------------------------------------
    // Binding ID Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_generate_binding_id_format() {
        let id = generate_binding_id("Given", "a server is running");
        assert_eq!(id.len(), 64);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_generate_binding_id_deterministic() {
        let id1 = generate_binding_id("When", "a client connects");
        let id2 = generate_binding_id("When", "a client connects");
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_generate_binding_id_different_kinds() {
        let given = generate_binding_id("Given", "something happens");
        let when = generate_binding_id("When", "something happens");
        assert_ne!(given, when, "different kinds should produce different IDs");
    }

    #[test]
    fn test_generate_binding_id_normalized() {
        // Same expression with different line endings should produce same ID
        let id1 = generate_binding_id("Given", "a\ntest");
        let id2 = generate_binding_id("Given", "a\r\ntest");
        assert_eq!(id1, id2);
    }

    // -------------------------------------------------------------------------
    // Feature Fingerprint Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_feature_fingerprint_deterministic() {
        let files = vec![
            ("a.feature", "Feature: A"),
            ("b.feature", "Feature: B"),
        ];
        let h1 = compute_feature_fingerprint(files.clone().into_iter());
        let h2 = compute_feature_fingerprint(files.into_iter());
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_feature_fingerprint_order_independent() {
        // Files should be sorted by path, so order of input shouldn't matter
        let files1 = vec![
            ("b.feature", "Feature: B"),
            ("a.feature", "Feature: A"),
        ];
        let files2 = vec![
            ("a.feature", "Feature: A"),
            ("b.feature", "Feature: B"),
        ];
        let h1 = compute_feature_fingerprint(files1.into_iter());
        let h2 = compute_feature_fingerprint(files2.into_iter());
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_feature_fingerprint_content_sensitive() {
        let files1 = vec![("a.feature", "Feature: A")];
        let files2 = vec![("a.feature", "Feature: B")];
        let h1 = compute_feature_fingerprint(files1.into_iter());
        let h2 = compute_feature_fingerprint(files2.into_iter());
        assert_ne!(h1, h2);
    }

    // -------------------------------------------------------------------------
    // Scenario Key Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_derive_scenario_key() {
        let key = derive_scenario_key("specs/features/smoke/test.feature", 42);
        assert_eq!(key, "specs/features/smoke/test.feature:L42");
    }

    #[test]
    fn test_derive_scenario_key_path_normalization() {
        // Backslashes should be converted
        let key = derive_scenario_key("specs\\features\\test.feature", 10);
        assert_eq!(key, "specs/features/test.feature:L10");

        // Leading ./ should be removed
        let key = derive_scenario_key("./specs/test.feature", 5);
        assert_eq!(key, "specs/test.feature:L5");
    }

    #[test]
    fn test_derive_scenario_outline_key() {
        let key = derive_scenario_outline_key("specs/auth/login.feature", 15, 0, 2);
        assert_eq!(key, "specs/auth/login.feature:L15:E0:R2");
    }

    // -------------------------------------------------------------------------
    // Golden Fixture Tests (per TODO.md Step 2)
    // -------------------------------------------------------------------------

    #[test]
    fn test_golden_unicode_korean() {
        // Korean text should normalize consistently
        let input = "한글 테스트";
        let normalized = normalize_string(input);
        assert_eq!(normalized, input); // Already NFC
    }

    #[test]
    fn test_golden_unicode_emoji() {
        // Emoji with variation selector
        let input = "test ❤️ emoji";
        let normalized = normalize_string(input);
        // Should preserve the emoji
        assert!(normalized.contains('❤'));
    }

    #[test]
    fn test_golden_complex_nfc() {
        // Ω can be Greek capital omega (U+03A9) or Ohm sign (U+2126)
        // NFC normalizes Ohm sign to Greek omega
        let ohm = "\u{2126}"; // Ohm sign
        let omega = "\u{03A9}"; // Greek capital omega
        assert_eq!(normalize_string(ohm), omega);
    }

    #[test]
    fn test_golden_mixed_newlines() {
        let input = "line1\r\nline2\rline3\nline4";
        let expected = "line1\nline2\nline3\nline4";
        assert_eq!(normalize_string(input), expected);
    }

    #[test]
    fn test_golden_json_unicode_keys() {
        let value = json!({"日本語": 1, "abc": 2});
        let encoded = canonical_json_encode(&value).unwrap();
        // ASCII sorts before CJK in Unicode code point order
        assert_eq!(encoded, r#"{"abc":2,"日本語":1}"#);
    }

    #[test]
    fn test_golden_binding_id_known_value() {
        // This is a golden fixture - the hash should never change
        let id = generate_binding_id("Given", "a server is running");
        // If this test fails, it means the hashing algorithm changed
        // Update this expected value only if intentionally changing the scheme
        assert_eq!(
            id,
            "479972f440a609dcdd70639c3820df553b4b7d0a47b563234a5bab31b4089f6d"
        );
    }

    #[test]
    fn test_golden_empty_string() {
        let normalized = normalize_string("");
        assert_eq!(normalized, "");

        let hash = blake3_256_lowerhex(b"");
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_golden_whitespace_preserved() {
        // Whitespace should be preserved (v1 does not collapse)
        let input = "a   b\t\tc";
        let normalized = normalize_string(input);
        assert_eq!(normalized, input);
    }

    // -------------------------------------------------------------------------
    // NPAP Type Struct Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_semantic_binding_serialization() {
        let binding = SemanticBinding {
            binding_id: "abc123".to_string(),
            kind: "Given".to_string(),
            expression: "a server is running".to_string(),
            signature: BindingSignature {
                captures_arity: 0,
                accepts_docstring: false,
                accepts_datatable: false,
            },
            impl_hash: "def456".to_string(),
        };

        let json = canonical_json_encode(&binding).unwrap();
        assert!(json.contains("\"binding_id\":\"abc123\""));
        assert!(json.contains("\"kind\":\"Given\""));
    }

    #[test]
    fn test_semantic_registry_hash_deterministic() {
        let bindings = vec![
            SemanticBinding {
                binding_id: "bbb".to_string(),
                kind: "When".to_string(),
                expression: "test".to_string(),
                signature: BindingSignature {
                    captures_arity: 0,
                    accepts_docstring: false,
                    accepts_datatable: false,
                },
                impl_hash: "hash1".to_string(),
            },
            SemanticBinding {
                binding_id: "aaa".to_string(),
                kind: "Given".to_string(),
                expression: "another".to_string(),
                signature: BindingSignature {
                    captures_arity: 1,
                    accepts_docstring: true,
                    accepts_datatable: false,
                },
                impl_hash: "hash2".to_string(),
            },
        ];

        let registry1 = SemanticStepRegistry::new(bindings.clone());
        let registry2 = SemanticStepRegistry::new(bindings);

        assert_eq!(registry1.step_registry_hash, registry2.step_registry_hash);
        assert_eq!(registry1.step_registry_hash.len(), 64);

        // Bindings should be sorted by binding_id
        assert_eq!(registry1.bindings[0].binding_id, "aaa");
        assert_eq!(registry1.bindings[1].binding_id, "bbb");
    }

    #[test]
    fn test_planned_step_payload_hash() {
        let step1 = PlannedStep::new(
            "Given".to_string(),
            "some step text".to_string(),
            "binding123".to_string(),
            vec!["capture1".to_string()],
            None,
            None,
        );

        let step2 = PlannedStep::new(
            "Given".to_string(),
            "some step text".to_string(),
            "binding123".to_string(),
            vec!["capture1".to_string()],
            None,
            None,
        );

        assert_eq!(step1.payload_hash, step2.payload_hash);
        assert_eq!(step1.payload_hash.len(), 64);
    }

    #[test]
    fn test_planned_step_payload_hash_with_docstring() {
        let step_without = PlannedStep::new(
            "Given".to_string(),
            "text".to_string(),
            "binding123".to_string(),
            vec![],
            None,
            None,
        );

        let step_with = PlannedStep::new(
            "Given".to_string(),
            "text".to_string(),
            "binding123".to_string(),
            vec![],
            Some("docstring content".to_string()),
            None,
        );

        assert_ne!(step_without.payload_hash, step_with.payload_hash);
    }

    #[test]
    fn test_resolved_plan_hash_deterministic() {
        let scenarios = vec![
            ResolvedScenario {
                scenario_key: "z:L10".to_string(),
                feature_path: "z.feature".to_string(),
                scenario_name: "Z test".to_string(),
                steps: vec![],
            },
            ResolvedScenario {
                scenario_key: "a:L5".to_string(),
                feature_path: "a.feature".to_string(),
                scenario_name: "A test".to_string(),
                steps: vec![],
            },
        ];

        let plan1 = ResolvedPlan::new(
            "ff_hash".to_string(),
            "sr_hash".to_string(),
            scenarios.clone(),
        );
        let plan2 = ResolvedPlan::new(
            "ff_hash".to_string(),
            "sr_hash".to_string(),
            scenarios,
        );

        assert_eq!(plan1.header.resolved_plan_hash, plan2.header.resolved_plan_hash);
        assert_eq!(plan1.header.resolved_plan_hash.len(), 64);

        // Scenarios should be sorted by scenario_key
        assert_eq!(plan1.scenarios[0].scenario_key, "a:L5");
        assert_eq!(plan1.scenarios[1].scenario_key, "z:L10");
    }

    #[test]
    fn test_run_report_hash() {
        let results = vec![
            ScenarioResult {
                scenario_key: "test:L1".to_string(),
                status: ScenarioStatus::Passed,
                steps: vec![],
            },
        ];

        let report = RunReport::new(
            "ff_hash".to_string(),
            "sr_hash".to_string(),
            "rp_hash".to_string(),
            results,
        );

        assert_eq!(report.header.run_report_hash.len(), 64);
        assert_eq!(report.header.npap_version, NPAP_VERSION);
    }

    #[test]
    fn test_scenario_status_serialization() {
        use serde_json::to_string;

        assert_eq!(to_string(&ScenarioStatus::Passed).unwrap(), "\"passed\"");
        assert_eq!(to_string(&ScenarioStatus::Failed).unwrap(), "\"failed\"");
        assert_eq!(to_string(&ScenarioStatus::Skipped).unwrap(), "\"skipped\"");
    }

    #[test]
    fn test_step_status_serialization() {
        use serde_json::to_string;

        assert_eq!(to_string(&StepStatus::Passed).unwrap(), "\"passed\"");
        assert_eq!(to_string(&StepStatus::Failed).unwrap(), "\"failed\"");
        assert_eq!(to_string(&StepStatus::Skipped).unwrap(), "\"skipped\"");
    }

    #[test]
    fn test_certification_roundtrip() {
        let cert = Certification {
            identity: CertificationIdentity {
                hash_contract_version: HASH_CONTRACT_VERSION.to_string(),
                feature_fingerprint_hash: "ff".to_string(),
                step_registry_hash: "sr".to_string(),
                resolved_plan_hash: "rp".to_string(),
            },
            metadata: CertificationMetadata {
                timestamp: "2025-01-16T00:00:00Z".to_string(),
                namako_version: "0.1.0".to_string(),
                npap_version: NPAP_VERSION,
                run_report_hash: "rr".to_string(),
            },
        };

        let json = serde_json::to_string(&cert).unwrap();
        let parsed: Certification = serde_json::from_str(&json).unwrap();

        assert_eq!(cert, parsed);
    }
}
