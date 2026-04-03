//! Formatting helpers for snapshot output.

use crate::types::{Change, ChangeType};

pub fn format_relative_time(started: &str) -> String {
    let dt = match chrono::DateTime::parse_from_rfc3339(started) {
        Ok(dt) => dt.with_timezone(&chrono::Utc),
        Err(_) => return started.to_string(),
    };
    let duration = chrono::Utc::now().signed_duration_since(dt);
    if duration.num_seconds() < 0 {
        return dt.format("%Y-%m-%d %H:%M").to_string();
    }
    if duration.num_minutes() < 1 {
        "just now".to_string()
    } else if duration.num_minutes() < 60 {
        format!("{}m ago", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{}h ago", duration.num_hours())
    } else if duration.num_days() < 7 {
        format!("{}d ago", duration.num_days())
    } else {
        dt.format("%Y-%m-%d %H:%M").to_string()
    }
}

pub fn truncate_command(command: &[String], max: usize) -> String {
    let full = command.join(" ");
    if full.len() <= max {
        return full;
    }
    let limit = max.saturating_sub(3);
    let truncated: String = full.chars().take(limit).collect();
    format!("{truncated}...")
}

pub fn format_change_counts(changes: &[Change]) -> String {
    format_change_counts_inner(changes, false)
}

pub fn format_change_counts_colored(changes: &[Change]) -> String {
    format_change_counts_inner(changes, true)
}

fn format_change_counts_inner(changes: &[Change], color: bool) -> String {
    if changes.is_empty() {
        return if color {
            dim("(no changes)")
        } else {
            "(no changes)".to_string()
        };
    }
    let mut created = 0usize;
    let mut modified = 0usize;
    let mut deleted = 0usize;
    for c in changes {
        match c.change_type {
            ChangeType::Created => created += 1,
            ChangeType::Modified | ChangeType::PermissionsChanged => modified += 1,
            ChangeType::Deleted => deleted += 1,
        }
    }
    let mut parts = Vec::new();
    if created > 0 {
        let s = format!("+{created}");
        parts.push(if color { green(&s) } else { s });
    }
    if modified > 0 {
        let s = format!("~{modified}");
        parts.push(if color { yellow(&s) } else { s });
    }
    if deleted > 0 {
        let s = format!("-{deleted}");
        parts.push(if color { red(&s) } else { s });
    }
    parts.join(" ")
}

pub fn dim(s: &str) -> String {
    format!("\x1b[2m{s}\x1b[0m")
}

pub fn green(s: &str) -> String {
    format!("\x1b[32m{s}\x1b[0m")
}

pub fn yellow(s: &str) -> String {
    format!("\x1b[33m{s}\x1b[0m")
}

pub fn red(s: &str) -> String {
    format!("\x1b[31m{s}\x1b[0m")
}
