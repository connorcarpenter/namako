//! Surface policy definitions for Tesaki missions.

use serde::{Deserialize, Serialize};

/// Lock state for an edit surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SurfaceLock {
    Locked,
    Unlocked,
}

/// Surface policy for a mission (Spec / Tests / SUT).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SurfacePolicy {
    pub spec: SurfaceLock,
    pub tests_bindings: SurfaceLock,
    pub sut: SurfaceLock,
}

/// Surface definition with glob patterns.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SurfaceDefinition {
    pub name: String,
    pub description: String,
    pub patterns: Vec<String>,
}

impl SurfaceDefinition {
    pub fn spec() -> Self {
        Self {
            name: "Spec".into(),
            description: "Feature files and spec artifacts".into(),
            patterns: vec!["test/specs/**/*.feature".into()],
        }
    }

    pub fn tests_bindings() -> Self {
        Self {
            name: "Tests/Bindings".into(),
            description: "Step bindings, harness, test infrastructure".into(),
            patterns: vec!["test/tests/**".into(), "test/harness/**".into()],
        }
    }

    pub fn sut() -> Self {
        Self {
            name: "SUT".into(),
            description: "System under test implementation".into(),
            patterns: vec!["src/**".into(), "client/**".into(), "server/**".into()],
        }
    }
}

impl SurfacePolicy {
    pub fn for_refine_spec() -> Self {
        Self {
            spec: SurfaceLock::Unlocked,
            tests_bindings: SurfaceLock::Locked,
            sut: SurfaceLock::Locked,
        }
    }

    pub fn for_structure_spec() -> Self {
        Self {
            spec: SurfaceLock::Unlocked,
            tests_bindings: SurfaceLock::Locked,
            sut: SurfaceLock::Locked,
        }
    }

    pub fn for_implement_tests() -> Self {
        Self {
            spec: SurfaceLock::Locked,
            tests_bindings: SurfaceLock::Unlocked,
            sut: SurfaceLock::Locked,
        }
    }

    pub fn for_implement_sut() -> Self {
        Self {
            spec: SurfaceLock::Locked,
            tests_bindings: SurfaceLock::Locked,
            sut: SurfaceLock::Unlocked,
        }
    }

    pub fn for_finalize() -> Self {
        Self {
            spec: SurfaceLock::Locked,
            tests_bindings: SurfaceLock::Locked,
            sut: SurfaceLock::Locked,
        }
    }

    pub fn to_markdown_table(&self) -> String {
        let mut content = String::new();
        content.push_str("| Surface | Policy |\n");
        content.push_str("|---------|--------|\n");
        content.push_str(&format!("| Spec | {} |\n", lock_label(self.spec)));
        content.push_str(&format!("| Tests/Bindings | {} |\n", lock_label(self.tests_bindings)));
        content.push_str(&format!("| SUT | {} |\n", lock_label(self.sut)));
        content
    }
}

fn lock_label(lock: SurfaceLock) -> &'static str {
    match lock {
        SurfaceLock::Locked => "LOCKED",
        SurfaceLock::Unlocked => "UNLOCKED",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policies_match_expectations() {
        assert_eq!(SurfacePolicy::for_refine_spec().spec, SurfaceLock::Unlocked);
        assert_eq!(SurfacePolicy::for_refine_spec().sut, SurfaceLock::Locked);
        assert_eq!(SurfacePolicy::for_implement_sut().sut, SurfaceLock::Unlocked);
    }

    #[test]
    fn markdown_table_includes_surfaces() {
        let policy = SurfacePolicy::for_implement_tests();
        let table = policy.to_markdown_table();
        assert!(table.contains("Spec"));
        assert!(table.contains("Tests/Bindings"));
        assert!(table.contains("SUT"));
    }

    #[test]
    fn surface_definition_defaults() {
        let spec = SurfaceDefinition::spec();
        assert!(spec.patterns.iter().any(|p| p.contains("specs")));
    }

    #[test]
    fn surface_lock_serialization() {
        let json = serde_json::to_string(&SurfaceLock::Locked).unwrap();
        assert_eq!(json, "\"LOCKED\"");
    }

    #[test]
    fn surface_policy_serialization() {
        let policy = SurfacePolicy::for_finalize();
        let json = serde_json::to_string(&policy).unwrap();
        assert!(json.contains("\"spec\""));
    }
}
