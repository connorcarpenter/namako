//! Persistent failure learning across sessions.
//!
//! This module tracks lessons learned from failures and successes, persisting them
//! to disk so that future sessions can learn from past attempts.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use anyhow::{Context, Result};

/// A lesson learned from a failure or success.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lesson {
    /// Unique identifier (UUID or hash)
    pub id: String,
    /// When this lesson was created (ISO 8601 timestamp)
    pub created: String,
    /// Issue key (e.g., scenario key)
    pub issue_key: String,
    /// Description of the failure mode
    pub failure_mode: String,
    /// Approaches that were attempted
    pub attempted_approaches: Vec<String>,
    /// What blocked progress (e.g., "spec surface locked")
    pub blocked_by: Option<String>,
    /// How it was resolved (filled when issue is resolved)
    pub resolution: Option<String>,
    /// Additional notes
    pub notes: Option<String>,
}

/// Database of lessons persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LessonsDatabase {
    /// Schema version
    pub version: u32,
    /// All lessons
    pub lessons: Vec<Lesson>,
}

impl Default for LessonsDatabase {
    fn default() -> Self {
        Self {
            version: 1,
            lessons: Vec::new(),
        }
    }
}

impl LessonsDatabase {
    /// Load lessons database from .tesaki/lessons.json
    pub fn load(spec_root: &Path) -> Result<Self> {
        let lessons_path = spec_root.join(".tesaki/lessons.json");
        
        if !lessons_path.exists() {
            return Ok(Self::default());
        }
        
        let content = fs::read_to_string(&lessons_path)
            .context("Failed to read lessons.json")?;
        
        let db: LessonsDatabase = serde_json::from_str(&content)
            .context("Failed to parse lessons.json")?;
        
        Ok(db)
    }
    
    /// Save lessons database to .tesaki/lessons.json
    pub fn save(&self, spec_root: &Path) -> Result<()> {
        let tesaki_dir = spec_root.join(".tesaki");
        fs::create_dir_all(&tesaki_dir)
            .context("Failed to create .tesaki directory")?;
        
        let lessons_path = tesaki_dir.join("lessons.json");
        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize lessons database")?;
        
        fs::write(&lessons_path, content)
            .context("Failed to write lessons.json")?;
        
        Ok(())
    }
    
    /// Add a new lesson to the database.
    pub fn add_lesson(&mut self, lesson: Lesson) {
        self.lessons.push(lesson);
    }
    
    /// Find all lessons for a specific target/issue.
    pub fn find_lessons_for_target(&self, target: &str) -> Vec<&Lesson> {
        self.lessons
            .iter()
            .filter(|l| l.issue_key == target)
            .collect()
    }
    
    /// Mark a lesson as resolved with the successful approach.
    pub fn mark_resolved(&mut self, id: &str, resolution: &str) {
        if let Some(lesson) = self.lessons.iter_mut().find(|l| l.id == id) {
            lesson.resolution = Some(resolution.to_string());
        }
    }
    
    /// Add an attempted approach to an existing lesson.
    pub fn add_attempted_approach(&mut self, issue_key: &str, approach: &str) -> bool {
        if let Some(lesson) = self.lessons.iter_mut().find(|l| l.issue_key == issue_key && l.resolution.is_none()) {
            if !lesson.attempted_approaches.contains(&approach.to_string()) {
                lesson.attempted_approaches.push(approach.to_string());
            }
            true
        } else {
            false
        }
    }
}

/// Create a new lesson from failure information.
pub fn create_lesson(
    issue_key: &str,
    failure_mode: &str,
    attempted_approach: Option<&str>,
    blocked_by: Option<&str>,
) -> Lesson {
    use uuid::Uuid;
    
    Lesson {
        id: Uuid::new_v4().to_string(),
        created: chrono::Utc::now().to_rfc3339(),
        issue_key: issue_key.to_string(),
        failure_mode: failure_mode.to_string(),
        attempted_approaches: attempted_approach
            .map(|a| vec![a.to_string()])
            .unwrap_or_default(),
        blocked_by: blocked_by.map(|s| s.to_string()),
        resolution: None,
        notes: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_lessons_database_default() {
        let db = LessonsDatabase::default();
        assert_eq!(db.version, 1);
        assert_eq!(db.lessons.len(), 0);
    }
    
    #[test]
    fn test_add_lesson() {
        let mut db = LessonsDatabase::default();
        let lesson = create_lesson(
            "feature:auth:login",
            "policy_violation",
            Some("tried to edit spec"),
            Some("spec surface locked"),
        );
        db.add_lesson(lesson);
        assert_eq!(db.lessons.len(), 1);
        assert_eq!(db.lessons[0].issue_key, "feature:auth:login");
    }
    
    #[test]
    fn test_find_lessons_for_target() {
        let mut db = LessonsDatabase::default();
        
        let lesson1 = create_lesson("feature:auth:login", "policy_violation", None, None);
        let lesson2 = create_lesson("feature:auth:logout", "no_progress", None, None);
        let lesson3 = create_lesson("feature:auth:login", "regression", None, None);
        
        db.add_lesson(lesson1);
        db.add_lesson(lesson2);
        db.add_lesson(lesson3);
        
        let found = db.find_lessons_for_target("feature:auth:login");
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|l| l.issue_key == "feature:auth:login"));
    }
    
    #[test]
    fn test_mark_resolved() {
        let mut db = LessonsDatabase::default();
        let lesson = create_lesson("feature:auth:login", "policy_violation", None, None);
        let id = lesson.id.clone();
        db.add_lesson(lesson);
        
        db.mark_resolved(&id, "Unlocked spec surface and fixed");
        assert_eq!(db.lessons[0].resolution, Some("Unlocked spec surface and fixed".to_string()));
    }
    
    #[test]
    fn test_add_attempted_approach() {
        let mut db = LessonsDatabase::default();
        let lesson = create_lesson(
            "feature:auth:login",
            "policy_violation",
            Some("approach 1"),
            None,
        );
        db.add_lesson(lesson);
        
        let added = db.add_attempted_approach("feature:auth:login", "approach 2");
        assert!(added);
        assert_eq!(db.lessons[0].attempted_approaches.len(), 2);
        
        // Don't add duplicates
        let added = db.add_attempted_approach("feature:auth:login", "approach 2");
        assert!(added);
        assert_eq!(db.lessons[0].attempted_approaches.len(), 2);
    }
    
    #[test]
    fn test_save_and_load() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let spec_root = temp_dir.path();
        
        let mut db = LessonsDatabase::default();
        let lesson = create_lesson("feature:auth:login", "policy_violation", None, None);
        db.add_lesson(lesson);
        
        db.save(spec_root)?;
        
        let loaded = LessonsDatabase::load(spec_root)?;
        assert_eq!(loaded.lessons.len(), 1);
        assert_eq!(loaded.lessons[0].issue_key, "feature:auth:login");
        
        Ok(())
    }
    
    #[test]
    fn test_load_missing_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let spec_root = temp_dir.path();
        
        let db = LessonsDatabase::load(spec_root)?;
        assert_eq!(db.lessons.len(), 0);
        
        Ok(())
    }
    
    #[test]
    fn test_no_add_approach_to_resolved() {
        let mut db = LessonsDatabase::default();
        let lesson = create_lesson("feature:auth:login", "policy_violation", Some("approach 1"), None);
        let id = lesson.id.clone();
        db.add_lesson(lesson);
        
        db.mark_resolved(&id, "Fixed");
        
        // Should not add to resolved lesson
        db.add_attempted_approach("feature:auth:login", "approach 2");
        assert_eq!(db.lessons[0].attempted_approaches.len(), 1);
    }
}
