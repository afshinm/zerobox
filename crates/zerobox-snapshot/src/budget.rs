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
                "snapshot walk exceeded entry limit ({entries} > {}). \
                 Add exclusion patterns with --snapshot-exclude.",
                self.max_entries
            );
        }
        if self.max_bytes > 0 && bytes > self.max_bytes {
            bail!(
                "snapshot walk exceeded byte limit ({bytes} > {} bytes). \
                 Add exclusion patterns with --snapshot-exclude.",
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
    fn entry_limit_exceeded() {
        let budget = WalkBudget {
            max_entries: 10,
            max_bytes: 0,
        };
        assert!(budget.check(10, 0).is_ok());
        assert!(budget.check(11, 0).is_err());
    }

    #[test]
    fn byte_limit_exceeded() {
        let budget = WalkBudget {
            max_entries: 0,
            max_bytes: 1000,
        };
        assert!(budget.check(1, 1000).is_ok());
        assert!(budget.check(1, 1001).is_err());
    }

    #[test]
    fn error_messages_reference_correct_flags() {
        let budget = WalkBudget {
            max_entries: 1,
            max_bytes: 0,
        };
        let err = budget.check(2, 0).unwrap_err().to_string();
        assert!(err.contains("--snapshot-exclude"), "got: {err}");
    }
}
