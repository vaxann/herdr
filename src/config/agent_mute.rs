//! `ui.muted_agents`: agents whose completions stay quiet.
//!
//! Entries match the agent name assigned with `herdr agent rename`. A plain
//! entry is an exact name; an entry prefixed with `re:` is a regular expression
//! searched anywhere in the name (anchor it with `^`/`$` when needed).

use std::collections::HashSet;

const REGEX_PREFIX: &str = "re:";

#[derive(Clone, Debug, Default)]
pub(crate) struct AgentMuteRules {
    exact: HashSet<String>,
    patterns: Vec<regex::Regex>,
}

impl AgentMuteRules {
    /// Compiles the configured entries, skipping invalid ones. Use
    /// [`muted_agents_diagnostics`] to report what was skipped.
    pub(crate) fn compile(entries: &[String]) -> Self {
        let mut rules = Self::default();
        for entry in entries {
            match parse_entry(entry) {
                Ok(Some(MuteEntry::Exact(name))) => {
                    rules.exact.insert(name);
                }
                Ok(Some(MuteEntry::Pattern(pattern))) => rules.patterns.push(pattern),
                Ok(None) | Err(_) => {}
            }
        }
        rules
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.exact.is_empty() && self.patterns.is_empty()
    }

    pub(crate) fn matches(&self, name: Option<&str>) -> bool {
        let Some(name) = name else {
            return false;
        };
        if self.is_empty() {
            return false;
        }
        self.exact.contains(name) || self.patterns.iter().any(|pattern| pattern.is_match(name))
    }
}

enum MuteEntry {
    Exact(String),
    Pattern(regex::Regex),
}

fn parse_entry(entry: &str) -> Result<Option<MuteEntry>, String> {
    let entry = entry.trim();
    if let Some(pattern) = entry.strip_prefix(REGEX_PREFIX) {
        if pattern.is_empty() {
            return Err(format!(
                "ui.muted_agents entry {entry:?} has an empty regular expression; ignored"
            ));
        }
        return regex::Regex::new(pattern)
            .map(|pattern| Some(MuteEntry::Pattern(pattern)))
            .map_err(|error| {
                format!("ui.muted_agents entry {entry:?} is not a valid regular expression ({error}); ignored")
            });
    }
    if entry.is_empty() {
        return Ok(None);
    }
    Ok(Some(MuteEntry::Exact(entry.to_ascii_lowercase())))
}

pub(crate) fn muted_agents_diagnostics(entries: &[String]) -> Vec<String> {
    entries
        .iter()
        .filter_map(|entry| parse_entry(entry).err())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn exact_entries_match_whole_names_only() {
        let rules = AgentMuteRules::compile(&entries(&["daily-report", " Nightly "]));
        assert!(rules.matches(Some("daily-report")));
        assert!(rules.matches(Some("nightly")));
        assert!(!rules.matches(Some("daily-report-2")));
        assert!(!rules.matches(None));
    }

    #[test]
    fn regex_entries_search_the_name() {
        let rules = AgentMuteRules::compile(&entries(&["re:^cron-", "re:-watch$"]));
        assert!(rules.matches(Some("cron-backup")));
        assert!(rules.matches(Some("logs-watch")));
        assert!(!rules.matches(Some("my-cron-job")));
        assert!(!rules.matches(Some("watcher")));
    }

    #[test]
    fn invalid_and_empty_entries_are_reported_and_skipped() {
        let configured = entries(&["re:(", "re:", "", "ok"]);
        let rules = AgentMuteRules::compile(&configured);
        assert!(rules.matches(Some("ok")));
        assert!(!rules.matches(Some("(")));
        let diagnostics = muted_agents_diagnostics(&configured);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        assert!(diagnostics[0].contains("not a valid regular expression"));
        assert!(diagnostics[1].contains("empty regular expression"));
    }

    #[test]
    fn empty_rules_match_nothing() {
        let rules = AgentMuteRules::compile(&[]);
        assert!(rules.is_empty());
        assert!(!rules.matches(Some("anything")));
    }
}
