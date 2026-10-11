//! Player identity resolution across replays.
//!
//! BW players often appear under different names: clan tags (`[KT]Flash`),
//! smurf names, character variations, or formatting differences. This module
//! normalizes names and provides grouping/matching utilities.
//!
//! ## Name normalization
//!
//! - Strip clan tags: `[KT]Flash` → `Flash`, `SKT1_Rain` → `Rain`
//! - Strip color codes and special characters
//! - Case-insensitive matching
//! - Trim whitespace
//!
//! ## Identity resolver
//!
//! Feed player appearances (name + race + optional context) and the resolver
//! groups them into canonical identities, handling common variations.

use std::collections::HashMap;

/// A normalized player name with metadata.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NormalizedPlayer {
    /// Original name as it appeared in the replay.
    pub original: String,
    /// Normalized name (clan tag stripped, trimmed, lowercased).
    pub normalized: String,
    /// Detected clan tag, if any.
    pub clan_tag: Option<String>,
}

/// Normalize a player name: strip clan tags, color codes, and whitespace.
pub fn normalize_name(name: &str) -> NormalizedPlayer {
    let trimmed = name.trim();

    // Detect and strip clan tags.
    let (clan_tag, base_name) = extract_clan_tag(trimmed);

    // Strip remaining special characters at boundaries.
    let cleaned = base_name
        .trim_matches(|c: char| !c.is_alphanumeric() && c != ' ')
        .trim();

    NormalizedPlayer {
        original: name.to_string(),
        normalized: cleaned.to_lowercase(),
        clan_tag,
    }
}

/// Extract a clan tag from a player name.
///
/// Supports common BW clan tag patterns:
/// - `[TAG]Name` → tag="TAG", name="Name"
/// - `TAG_Name` → tag="TAG", name="Name" (2-4 uppercase letters before underscore)
/// - `Name` → no tag
fn extract_clan_tag(name: &str) -> (Option<String>, &str) {
    // Pattern: [TAG]Name
    if name.starts_with('[')
        && let Some(end) = name.find(']')
    {
        let tag = &name[1..end];
        let rest = name[end + 1..].trim_start();
        if !tag.is_empty() && !rest.is_empty() {
            return (Some(tag.to_string()), rest);
        }
    }

    // Pattern: (TAG)Name
    if name.starts_with('(')
        && let Some(end) = name.find(')')
    {
        let tag = &name[1..end];
        let rest = name[end + 1..].trim_start();
        if tag.len() <= 5
            && !tag.is_empty()
            && !rest.is_empty()
            && tag.chars().all(|c| c.is_alphanumeric())
        {
            return (Some(tag.to_string()), rest);
        }
    }

    // Pattern: TAG_Name (2-4 uppercase letters + underscore).
    if let Some(us) = name.find('_') {
        let prefix = &name[..us];
        let suffix = &name[us + 1..];
        if prefix.len() >= 2
            && prefix.len() <= 4
            && prefix
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            && !suffix.is_empty()
        {
            return (Some(prefix.to_string()), suffix);
        }
    }

    (None, name)
}

/// An identity resolver that groups player appearances into canonical identities.
#[derive(Debug, Default)]
pub struct IdentityResolver {
    /// Normalized name → list of original appearances.
    appearances: HashMap<String, Vec<PlayerAppearance>>,
}

/// A single appearance of a player in a replay.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlayerAppearance {
    /// Original name in the replay.
    pub name: String,
    /// Normalized name.
    pub normalized: String,
    /// Clan tag if detected.
    pub clan_tag: Option<String>,
    /// Race played.
    pub race: String,
}

/// A resolved player identity grouping multiple appearances.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlayerIdentity {
    /// Canonical name (most common normalized form).
    pub canonical_name: String,
    /// All known name variations seen.
    pub aliases: Vec<String>,
    /// All known clan tags.
    pub clan_tags: Vec<String>,
    /// Races played (with frequency).
    pub races: HashMap<String, u32>,
    /// Total appearances (games).
    pub game_count: u32,
}

impl IdentityResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a player appearance.
    pub fn add(&mut self, name: &str, race: &str) {
        let norm = normalize_name(name);
        let appearance = PlayerAppearance {
            name: name.to_string(),
            normalized: norm.normalized.clone(),
            clan_tag: norm.clan_tag,
            race: race.to_string(),
        };
        self.appearances
            .entry(norm.normalized)
            .or_default()
            .push(appearance);
    }

    /// Resolve all accumulated appearances into player identities.
    pub fn resolve(&self) -> Vec<PlayerIdentity> {
        self.appearances
            .iter()
            .map(|(canonical, appearances)| {
                let mut aliases: Vec<String> = appearances
                    .iter()
                    .map(|a| a.name.clone())
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();
                aliases.sort();

                let mut clan_tags: Vec<String> = appearances
                    .iter()
                    .filter_map(|a| a.clan_tag.clone())
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();
                clan_tags.sort();

                let mut races: HashMap<String, u32> = HashMap::new();
                for a in appearances {
                    *races.entry(a.race.clone()).or_insert(0) += 1;
                }

                PlayerIdentity {
                    canonical_name: canonical.clone(),
                    aliases,
                    clan_tags,
                    races,
                    game_count: appearances.len() as u32,
                }
            })
            .collect()
    }

    /// Look up a name and return the canonical form.
    pub fn lookup(&self, name: &str) -> Option<&str> {
        let norm = normalize_name(name);
        self.appearances
            .get_key_value(&norm.normalized)
            .map(|(k, _)| k.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Name normalization --

    #[test]
    fn test_normalize_plain() {
        let n = normalize_name("Flash");
        assert_eq!(n.normalized, "flash");
        assert!(n.clan_tag.is_none());
    }

    #[test]
    fn test_normalize_bracket_tag() {
        let n = normalize_name("[KT]Flash");
        assert_eq!(n.normalized, "flash");
        assert_eq!(n.clan_tag.as_deref(), Some("KT"));
    }

    #[test]
    fn test_normalize_paren_tag() {
        let n = normalize_name("(SKT)Rain");
        assert_eq!(n.normalized, "rain");
        assert_eq!(n.clan_tag.as_deref(), Some("SKT"));
    }

    #[test]
    fn test_normalize_underscore_tag() {
        let n = normalize_name("SKT1_Rain");
        assert_eq!(n.normalized, "rain");
        assert_eq!(n.clan_tag.as_deref(), Some("SKT1"));
    }

    #[test]
    fn test_normalize_no_false_tag() {
        // "The_Great" — "The" has lowercase, not a clan tag.
        let n = normalize_name("The_Great");
        assert!(n.clan_tag.is_none());
        assert_eq!(n.normalized, "the_great");
    }

    #[test]
    fn test_normalize_whitespace() {
        let n = normalize_name("  Flash  ");
        assert_eq!(n.normalized, "flash");
    }

    #[test]
    fn test_normalize_special_chars() {
        let n = normalize_name("~Flash~");
        assert_eq!(n.normalized, "flash");
    }

    // -- Identity resolver --

    #[test]
    fn test_resolver_groups_same_player() {
        let mut r = IdentityResolver::new();
        r.add("[KT]Flash", "T");
        r.add("Flash", "T");
        r.add("[KT]Flash", "T");

        let identities = r.resolve();
        assert_eq!(identities.len(), 1);
        assert_eq!(identities[0].canonical_name, "flash");
        assert_eq!(identities[0].game_count, 3);
        assert!(identities[0].aliases.contains(&"[KT]Flash".to_string()));
        assert!(identities[0].aliases.contains(&"Flash".to_string()));
        assert!(identities[0].clan_tags.contains(&"KT".to_string()));
    }

    #[test]
    fn test_resolver_separates_different_players() {
        let mut r = IdentityResolver::new();
        r.add("Flash", "T");
        r.add("Jaedong", "Z");

        let identities = r.resolve();
        assert_eq!(identities.len(), 2);
    }

    #[test]
    fn test_resolver_tracks_races() {
        let mut r = IdentityResolver::new();
        r.add("Flash", "T");
        r.add("Flash", "T");
        r.add("Flash", "P"); // Rare off-race game.

        let identities = r.resolve();
        assert_eq!(identities[0].races["T"], 2);
        assert_eq!(identities[0].races["P"], 1);
    }

    #[test]
    fn test_lookup() {
        let mut r = IdentityResolver::new();
        r.add("[KT]Flash", "T");

        assert_eq!(r.lookup("Flash"), Some("flash"));
        assert_eq!(r.lookup("[KT]Flash"), Some("flash"));
        assert!(r.lookup("Jaedong").is_none());
    }
}
