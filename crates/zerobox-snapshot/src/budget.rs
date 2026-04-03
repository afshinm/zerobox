//! Walk budget to prevent snapshotting massive directories by accident.

use anyhow::{Result, bail};

/// 0 means unlimited.
#[derive(Debug, Clone)]
pub struct WalkBudget {
    pub max_entries: usize,
    pub max_bytes: u64,
}

impl Default for WalkBudget {
    fn default() -> Self {
        Self {
            max_entries: 300_000,
            max_bytes: 2 * 1024 * 1024 * 1024,
        }
    }
}

impl WalkBudget {
    /// Errors with actionable advice when limits are exceeded.
    pub fn check(&self, entries: usize, bytes: u64) -> Result<()> {
        if self.max_entries > 0 && entries > self.max_entries {
            bail!(
                "rollback walk exceeded entry limit ({entries} > {}). \
                 Add exclusion patterns with --rollback-exclude or disable with --no-rollback.",
                self.max_entries
            );
        }
        if self.max_bytes > 0 && bytes > self.max_bytes {
            bail!(
                "rollback walk exceeded byte limit ({bytes} > {} bytes). \
                 Add exclusion patterns with --rollback-exclude or disable with --no-rollback.",
                self.max_bytes
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_budget_allows_small_walks() {
        let budget = WalkBudget::default();
        assert!(budget.check(100, 1024).is_ok());
    }

    #[test]
    fn entry_limit_exceeded() {
        let budget = WalkBudget {
            max_entries: 10,
            max_bytes: 0,
        };
        assert!(budget.check(11, 0).is_err());
    }

    #[test]
    fn byte_limit_exceeded() {
        let budget = WalkBudget {
            max_entries: 0,
            max_bytes: 1000,
        };
        assert!(budget.check(1, 1001).is_err());
    }

    #[test]
    fn unlimited_budget_always_ok() {
        let budget = WalkBudget {
            max_entries: 0,
            max_bytes: 0,
        };
        assert!(budget.check(999_999, 999_999_999).is_ok());
    }
}
