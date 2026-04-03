//! File exclusion: .gitignore, component patterns, globs, and force-include overrides.

use std::path::Path;

use anyhow::Result;
use globset::{Glob, GlobSet, GlobSetBuilder};

#[derive(Debug, Clone)]
pub struct ExclusionConfig {
    pub use_gitignore: bool,
    /// Without `/`: match exact path components. With `/`: substring match.
    pub exclude_patterns: Vec<String>,
    /// Matched against filename only.
    pub exclude_globs: Vec<String>,
    /// Highest priority: always included regardless of other rules.
    pub force_include: Vec<String>,
}

impl Default for ExclusionConfig {
    fn default() -> Self {
        Self {
            use_gitignore: true,
            exclude_patterns: Vec::new(),
            exclude_globs: Vec::new(),
            force_include: Vec::new(),
        }
    }
}

/// Directories that are regenerable and would corrupt if partially restored.
pub fn default_exclusions() -> Vec<String> {
    [
        ".git",
        ".hg",
        ".svn",
        "node_modules",
        "__pycache__",
        ".venv",
        "target",
        "build",
        "dist",
        "out",
        ".DS_Store",
        "Thumbs.db",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

/// Compiled filter with pre-built glob sets for fast matching.
#[derive(Clone)]
pub struct ExclusionFilter {
    exclude_patterns: Vec<String>,
    exclude_globs: Option<GlobSet>,
    force_include: Vec<String>,
}

impl ExclusionFilter {
    pub fn new(config: &ExclusionConfig) -> Result<Self> {
        let exclude_globs = if config.exclude_globs.is_empty() {
            None
        } else {
            let mut builder = GlobSetBuilder::new();
            for pattern in &config.exclude_globs {
                builder.add(Glob::new(pattern)?);
            }
            Some(builder.build()?)
        };

        Ok(Self {
            exclude_patterns: config.exclude_patterns.clone(),
            exclude_globs,
            force_include: config.force_include.clone(),
        })
    }

    /// Priority: force-include > exclude patterns > exclude globs.
    /// .gitignore is handled by the `ignore` crate walker, not here.
    pub fn is_excluded(&self, path: &Path) -> bool {
        if self.matches_force_include(path) {
            return false;
        }
        if self.matches_exclude_patterns(path) {
            return true;
        }
        if self.matches_exclude_globs(path) {
            return true;
        }
        false
    }

    fn matches_exclude_patterns(&self, path: &Path) -> bool {
        matches_any_pattern(path, &self.exclude_patterns)
    }

    fn matches_exclude_globs(&self, path: &Path) -> bool {
        if let Some(ref globs) = self.exclude_globs
            && let Some(filename) = path.file_name()
        {
            return globs.is_match(filename);
        }
        false
    }

    fn matches_force_include(&self, path: &Path) -> bool {
        matches_any_pattern(path, &self.force_include)
    }
}

/// With `/`: substring match on full path. Without: exact component match.
fn matches_any_pattern(path: &Path, patterns: &[String]) -> bool {
    for pattern in patterns {
        if pattern.contains('/') {
            if path.to_string_lossy().contains(pattern.as_str()) {
                return true;
            }
        } else {
            for component in path.components() {
                if let std::path::Component::Normal(c) = component
                    && c.to_string_lossy() == *pattern
                {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_filter(patterns: Vec<&str>) -> ExclusionFilter {
        let config = ExclusionConfig {
            use_gitignore: false,
            exclude_patterns: patterns.into_iter().map(String::from).collect(),
            exclude_globs: Vec::new(),
            force_include: Vec::new(),
        };
        ExclusionFilter::new(&config).unwrap()
    }

    #[test]
    fn component_pattern_matches() {
        let filter = make_filter(vec!["node_modules", ".DS_Store"]);
        assert!(filter.is_excluded(&PathBuf::from("project/node_modules/pkg/index.js")));
        assert!(filter.is_excluded(&PathBuf::from("a/.DS_Store")));
    }

    #[test]
    fn slash_pattern_matches_as_substring() {
        let filter = make_filter(vec![".git/objects"]);
        assert!(filter.is_excluded(&PathBuf::from("project/.git/objects/ab/cdef")));
        assert!(!filter.is_excluded(&PathBuf::from("project/.git/config")));
    }

    #[test]
    fn normal_files_not_excluded() {
        let filter = make_filter(vec!["node_modules"]);
        assert!(!filter.is_excluded(&PathBuf::from("src/main.rs")));
        assert!(!filter.is_excluded(&PathBuf::from("README.md")));
    }

    #[test]
    fn empty_patterns_excludes_nothing() {
        let filter = make_filter(vec![]);
        assert!(!filter.is_excluded(&PathBuf::from("anything/at/all")));
    }

    #[test]
    fn force_include_overrides_patterns() {
        let config = ExclusionConfig {
            use_gitignore: false,
            exclude_patterns: vec!["build".to_string()],
            exclude_globs: Vec::new(),
            force_include: vec!["build".to_string()],
        };
        let filter = ExclusionFilter::new(&config).unwrap();
        assert!(!filter.is_excluded(&PathBuf::from("project/build/output.js")));
    }

    #[test]
    fn glob_pattern_matches_filename() {
        let config = ExclusionConfig {
            use_gitignore: false,
            exclude_patterns: Vec::new(),
            exclude_globs: vec!["*.tmp".to_string()],
            force_include: Vec::new(),
        };
        let filter = ExclusionFilter::new(&config).unwrap();
        assert!(filter.is_excluded(&PathBuf::from("project/data.tmp")));
        assert!(!filter.is_excluded(&PathBuf::from("project/data.txt")));
    }

    #[test]
    fn force_include_matches_path_component() {
        let config = ExclusionConfig {
            use_gitignore: false,
            exclude_patterns: vec!["vendor".to_string()],
            exclude_globs: Vec::new(),
            force_include: vec!["important".to_string()],
        };
        let filter = ExclusionFilter::new(&config).unwrap();
        assert!(!filter.is_excluded(&PathBuf::from("vendor/important/lib.rs")));
    }

    #[test]
    fn force_include_rejects_substring_match() {
        let config = ExclusionConfig {
            use_gitignore: false,
            exclude_patterns: vec!["vendor".to_string()],
            exclude_globs: Vec::new(),
            force_include: vec!["app".to_string()],
        };
        let filter = ExclusionFilter::new(&config).unwrap();
        assert!(filter.is_excluded(&PathBuf::from("vendor/myapp/file.txt")));
        assert!(!filter.is_excluded(&PathBuf::from("vendor/app/file.txt")));
    }
}
