//! Small pure-logic helpers shared by the `ee` and `eee` binaries.
//!
//! Kept separate from the binaries mainly so it's unit-testable without a
//! terminal. `rr` has its own file formats (composite keys, tab-embedded
//! commands) and is not a fit for these — its pure helpers are tested in
//! `rr.rs` directly.

use std::collections::HashMap;
use std::io::BufRead;

/// Parse "name\tcount" lines into a counts map. Malformed lines are skipped.
pub fn parse_counts<R: BufRead>(reader: R) -> HashMap<String, u64> {
    reader
        .lines()
        .map_while(Result::ok)
        .filter_map(|l| {
            let mut parts = l.splitn(2, '\t');
            let name = parts.next()?.to_string();
            let count = parts.next()?.parse::<u64>().ok()?;
            Some((name, count))
        })
        .collect()
}

/// Serialize a counts map back to "name\tcount\n..." form.
pub fn format_counts(counts: &HashMap<String, u64>) -> String {
    counts
        .iter()
        .map(|(name, count)| format!("{}\t{}", name, count))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// Parse newline-delimited MRU history, dropping blank lines.
pub fn parse_history<R: BufRead>(reader: R) -> Vec<String> {
    reader
        .lines()
        .map_while(Result::ok)
        .filter(|l| !l.trim().is_empty())
        .collect()
}

/// Serialize an MRU history list back to newline-delimited form.
pub fn format_history(history: &[String]) -> String {
    history.join("\n") + "\n"
}

/// Move `item` to the front of `history`, removing any existing occurrence.
pub fn record_mru(history: &mut Vec<String>, item: &str) {
    history.retain(|h| h != item);
    history.insert(0, item.to_string());
}

/// Case-insensitive substring filter over `items`, keyed by `key`.
pub fn filter_by_substring<'a, T, F>(items: &'a [T], query: &str, key: F) -> Vec<&'a T>
where
    F: Fn(&T) -> &str,
{
    let q = query.to_lowercase();
    if q.is_empty() {
        items.iter().collect()
    } else {
        items
            .iter()
            .filter(|i| key(i).to_lowercase().contains(&q))
            .collect()
    }
}

/// Order `entries` MRU-first: history entries that still exist, in history
/// order, followed by any remaining entries in their original order.
pub fn mru_order(entries: &[String], history: &[String]) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    for h in history {
        if entries.contains(h) {
            result.push(h.clone());
        }
    }
    for e in entries {
        if !result.contains(e) {
            result.push(e.clone());
        }
    }
    result
}

/// Parse "timestamp\tname" log lines, counting only entries at or after `cutoff`
/// (unix seconds). Used for `eee`'s recency-windowed sort.
pub fn parse_windowed_counts<R: BufRead>(reader: R, cutoff: u64) -> HashMap<String, u64> {
    let mut counts = HashMap::new();
    for line in reader.lines().map_while(Result::ok) {
        let mut parts = line.splitn(2, '\t');
        if let (Some(ts_str), Some(name)) = (parts.next(), parts.next()) {
            if let Ok(ts) = ts_str.parse::<u64>() {
                if ts >= cutoff {
                    *counts.entry(name.to_string()).or_insert(0) += 1;
                }
            }
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_counts_reads_valid_lines() {
        let input = "foo\t3\nbar\t7\n";
        let counts = parse_counts(input.as_bytes());
        assert_eq!(counts.get("foo"), Some(&3));
        assert_eq!(counts.get("bar"), Some(&7));
        assert_eq!(counts.len(), 2);
    }

    #[test]
    fn parse_counts_skips_malformed_lines() {
        let input = "foo\tnotanumber\nbar\n\tbaz\nok\t1\n";
        let counts = parse_counts(input.as_bytes());
        assert_eq!(counts.len(), 1);
        assert_eq!(counts.get("ok"), Some(&1));
    }

    #[test]
    fn counts_round_trip() {
        let mut counts = HashMap::new();
        counts.insert("a".to_string(), 1u64);
        counts.insert("b".to_string(), 42u64);
        let formatted = format_counts(&counts);
        let parsed = parse_counts(formatted.as_bytes());
        assert_eq!(parsed, counts);
    }

    #[test]
    fn parse_history_drops_blank_lines() {
        let input = "one\n\ntwo\n   \nthree\n";
        let history = parse_history(input.as_bytes());
        assert_eq!(history, vec!["one", "two", "three"]);
    }

    #[test]
    fn history_round_trip() {
        let history = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let formatted = format_history(&history);
        let parsed = parse_history(formatted.as_bytes());
        assert_eq!(parsed, history);
    }

    #[test]
    fn record_mru_moves_existing_item_to_front() {
        let mut history = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        record_mru(&mut history, "b");
        assert_eq!(history, vec!["b", "a", "c"]);
    }

    #[test]
    fn record_mru_inserts_new_item_at_front() {
        let mut history = vec!["a".to_string(), "b".to_string()];
        record_mru(&mut history, "z");
        assert_eq!(history, vec!["z", "a", "b"]);
    }

    #[test]
    fn record_mru_dedupes_repeats() {
        let mut history = vec!["a".to_string()];
        record_mru(&mut history, "a");
        record_mru(&mut history, "a");
        assert_eq!(history, vec!["a".to_string()]);
    }

    #[test]
    fn filter_by_substring_empty_query_returns_all() {
        let items = vec!["Alpha".to_string(), "beta".to_string()];
        let result = filter_by_substring(&items, "", |s| s.as_str());
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn filter_by_substring_is_case_insensitive() {
        let items = vec!["Alpha".to_string(), "beta".to_string(), "gamma".to_string()];
        let result = filter_by_substring(&items, "ETA", |s| s.as_str());
        assert_eq!(result, vec![&"beta".to_string()]);
    }

    #[test]
    fn filter_by_substring_no_match_returns_empty() {
        let items = vec!["Alpha".to_string()];
        let result = filter_by_substring(&items, "zzz", |s| s.as_str());
        assert!(result.is_empty());
    }

    #[test]
    fn mru_order_puts_history_first_then_remaining() {
        let entries = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ];
        let history = vec!["c".to_string(), "a".to_string()];
        let result = mru_order(&entries, &history);
        assert_eq!(result, vec!["c", "a", "b", "d"]);
    }

    #[test]
    fn mru_order_ignores_history_entries_no_longer_present() {
        let entries = vec!["a".to_string(), "b".to_string()];
        let history = vec!["ghost".to_string(), "b".to_string()];
        let result = mru_order(&entries, &history);
        assert_eq!(result, vec!["b", "a"]);
    }

    #[test]
    fn mru_order_empty_history_preserves_entry_order() {
        let entries = vec!["x".to_string(), "y".to_string()];
        let result = mru_order(&entries, &[]);
        assert_eq!(result, entries);
    }

    #[test]
    fn parse_windowed_counts_excludes_entries_before_cutoff() {
        let input = "100\told\n200\tnew\n300\tnew\n";
        let counts = parse_windowed_counts(input.as_bytes(), 200);
        assert_eq!(counts.get("old"), None);
        assert_eq!(counts.get("new"), Some(&2));
    }

    #[test]
    fn parse_windowed_counts_skips_malformed_lines() {
        let input = "notanumber\tname\n200\tname\n";
        let counts = parse_windowed_counts(input.as_bytes(), 0);
        assert_eq!(counts.get("name"), Some(&1));
    }
}
