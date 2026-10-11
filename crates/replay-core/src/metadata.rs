//! Game metadata derived from replay data.
//!
//! Provides matchup detection, map name normalization, winner detection,
//! and other high-level game information useful for stats and indexing.

use crate::command::{Command, GameCommand};
use crate::header::{Header, Race};

/// Detected matchup between two players.
///
/// For 1v1 games, this is the standard notation (e.g., "TvZ").
/// Races are sorted alphabetically: P < T < Z, so it's always
/// "PvT" not "TvP", "PvZ" not "ZvP", "TvZ" not "ZvT".
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize)]
pub struct Matchup {
    /// Canonical matchup string: "TvZ", "PvT", "ZvP", "TvT", "PvP", "ZvZ".
    pub code: String,
    /// Whether this is a mirror matchup.
    pub mirror: bool,
}

/// Detected game result.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum GameResult {
    /// A player won (the other left/disconnected).
    Winner { player_id: u8, player_name: String },
    /// Could not determine a winner (e.g., both left, or no LeaveGame command).
    Unknown,
}

/// High-level game metadata derived from a parsed replay.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GameMetadata {
    /// Detected matchup (e.g., "TvZ"). None if not a 1v1 or races unknown.
    pub matchup: Option<Matchup>,
    /// Normalized map name (stripped of version numbers, trimmed).
    pub map_name: String,
    /// Raw map name from the replay header.
    pub map_name_raw: String,
    /// Detected game result.
    pub result: GameResult,
    /// Game duration in real seconds (at Fastest speed).
    pub duration_secs: f64,
    /// Whether this is a 1v1 game.
    pub is_1v1: bool,
    /// Number of active (human/computer) players.
    pub player_count: usize,
}

/// Extract game metadata from a replay header and command stream.
pub fn extract_metadata(header: &Header, commands: &[GameCommand]) -> GameMetadata {
    let is_1v1 = header.players.len() == 2;
    let matchup = if is_1v1 {
        detect_matchup(&header.players[0].race, &header.players[1].race)
    } else {
        None
    };

    let map_name = normalize_map_name(&header.map_name);
    let result = detect_winner(header, commands);
    let duration_secs = header.frame_count as f64 / 23.81;

    GameMetadata {
        matchup,
        map_name,
        map_name_raw: header.map_name.clone(),
        result,
        duration_secs,
        is_1v1,
        player_count: header.players.len(),
    }
}

/// Detect matchup from two player races.
///
/// Returns canonical form where races are sorted: P < T < Z.
fn detect_matchup(race1: &Race, race2: &Race) -> Option<Matchup> {
    let code1 = race_code(race1)?;
    let code2 = race_code(race2)?;

    let mirror = code1 == code2;
    let (a, b) = if code1 <= code2 {
        (code1, code2)
    } else {
        (code2, code1)
    };

    Some(Matchup {
        code: format!("{a}v{b}"),
        mirror,
    })
}

fn race_code(race: &Race) -> Option<&'static str> {
    match race {
        Race::Terran => Some("T"),
        Race::Protoss => Some("P"),
        Race::Zerg => Some("Z"),
        Race::Unknown(_) => None,
    }
}

/// Detect the winner by finding which player's opponent left first.
///
/// In competitive BW, the loser issues a LeaveGame command. The player
/// who did NOT leave (or left last) is the winner.
fn detect_winner(header: &Header, commands: &[GameCommand]) -> GameResult {
    if header.players.len() != 2 {
        return GameResult::Unknown;
    }

    // Find LeaveGame commands, sorted by frame.
    let mut leaves: Vec<(u32, u8)> = commands
        .iter()
        .filter(|c| matches!(c.command, Command::LeaveGame { .. }))
        .map(|c| (c.frame, c.player_id))
        .collect();
    leaves.sort_by_key(|&(frame, _)| frame);

    if leaves.is_empty() {
        return GameResult::Unknown;
    }

    // The first player to leave is the loser.
    let loser_id = leaves[0].1;

    // Find the winner (the other player).
    let winner = header.players.iter().find(|p| p.player_id != loser_id);

    match winner {
        Some(p) => GameResult::Winner {
            player_id: p.player_id,
            player_name: p.name.clone(),
        },
        None => GameResult::Unknown,
    }
}

/// League/ladder tags that map makers or ladders put around map names
/// (`"| iCCup | Python"`). Compared case-insensitively against whole words.
const MAP_TAG_WORDS: &[&str] = &["iccup", "wgt", "wgtour", "bwcl", "kespa"];

/// Normalize a map name for consistent matching across replays.
///
/// - Strips StarCraft colour/formatting control characters
/// - Strips league tags like `"| iCCup |"` or `"[iCCup]"`
/// - Strips a leading player count like `"(2)"`
/// - Strips version suffixes, separated (`"1.3"`, `"v2.1"`, `"ver 1.2"`) or
///   attached to the name when they contain a dot (`"Transistor1.2"`)
/// - Collapses whitespace
///
/// The raw name is kept in [`GameMetadata::map_name_raw`].
///
/// Examples:
/// - "| iCCup | Python 1.3" → "Python"
/// - "Transistor1.2" → "Transistor"
/// - "(4)Fighting Spirit 1.3" → "Fighting Spirit"
/// - "Circuit Breaker v2.1" → "Circuit Breaker"
pub fn normalize_map_name(name: &str) -> String {
    // Control characters (colour codes) and tag delimiters become spaces.
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '|' | '[' | ']' | '{' | '}') {
                ' '
            } else {
                c
            }
        })
        .collect();

    // Drop league tag words, also when wrapped in parentheses: "(iCCup)".
    let words: Vec<&str> = cleaned
        .split_whitespace()
        .filter(|w| {
            let bare = w.trim_matches(|c: char| !c.is_alphanumeric());
            !MAP_TAG_WORDS.iter().any(|t| bare.eq_ignore_ascii_case(t))
        })
        .collect();
    let mut s = words.join(" ");

    // Strip leading player count prefix like "(2)", "(4)", "(8)".
    if s.starts_with('(')
        && let Some(end) = s.find(')')
    {
        let inside = &s[1..end];
        if !inside.is_empty() && inside.chars().all(|c| c.is_ascii_digit()) {
            s = s[end + 1..].trim_start().to_string();
        }
    }

    s = strip_version_suffix(&s);
    let s = s
        .trim_matches(|c: char| c.is_whitespace() || matches!(c, '-' | '_' | ':' | '.'))
        .to_string();

    if s.is_empty() {
        // Nothing left (e.g. the name was only a tag): fall back to the
        // cleaned raw name rather than an empty string.
        return cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    s
}

/// Remove a trailing version from a map name (see [`normalize_map_name`]).
fn strip_version_suffix(s: &str) -> String {
    let trimmed = s.trim_end();

    // Separated suffix: "Name 1.3", "Name v2", "Name ver 1.2", "Name_1.3".
    if let Some(pos) = trimmed.rfind([' ', '_', '-']) {
        let suffix = &trimmed[pos + 1..];
        if is_version_suffix(suffix) {
            let head = trimmed[..pos].trim_end();
            // "Name ver 1.2": drop a dangling "ver"/"v" word too.
            let head = head
                .strip_suffix(" ver")
                .or_else(|| head.strip_suffix(" Ver"))
                .or_else(|| head.strip_suffix(" v"))
                .or_else(|| head.strip_suffix(" V"))
                .unwrap_or(head);
            if !head.trim().is_empty() {
                return head.to_string();
            }
        }
    }

    // Attached suffix: "Transistor1.2", "Polypoidv1.65". Requires a dot so
    // names that end in a number ("Neo Sylphid2") are left alone.
    let bytes = trimmed.as_bytes();
    let mut start = trimmed.len();
    // Optional trailing letter ("1.3a").
    if start > 0 && bytes[start - 1].is_ascii_alphabetic() {
        start -= 1;
    }
    let digits_end = start;
    while start > 0 && (bytes[start - 1].is_ascii_digit() || bytes[start - 1] == b'.') {
        start -= 1;
    }
    let version = &trimmed[start..digits_end];
    if version.contains('.')
        && version.starts_with(|c: char| c.is_ascii_digit())
        && version.ends_with(|c: char| c.is_ascii_digit())
        && start > 0
    {
        let mut head = &trimmed[..start];
        if let Some(h) = head.strip_suffix(['v', 'V'])
            && h.ends_with(|c: char| c.is_alphabetic() || c == ' ')
        {
            head = h;
        }
        if head.ends_with(|c: char| c.is_alphabetic()) {
            return head.to_string();
        }
    }

    trimmed.to_string()
}

/// Check if a string looks like a version suffix ("1.3", "v2.1", "ver1.2",
/// "1.3a", "2").
fn is_version_suffix(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    let lower = s.to_ascii_lowercase();
    let s = lower
        .strip_prefix("ver.")
        .or_else(|| lower.strip_prefix("ver"))
        .or_else(|| lower.strip_prefix('v'))
        .unwrap_or(&lower);

    if s.is_empty() {
        return false;
    }

    // Must start with a digit.
    if !s.starts_with(|c: char| c.is_ascii_digit()) {
        return false;
    }

    // Allow: digits, dots, and optional trailing letter.
    let mut has_digit = false;
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_digit() {
            has_digit = true;
        } else if c == '.' {
            // dot must be between digits
        } else if c.is_ascii_alphabetic() && i == s.len() - 1 {
            // trailing letter like "1.3a"
        } else {
            return false;
        }
    }

    has_digit
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::*;

    fn make_player(id: u8, name: &str, race: Race) -> Player {
        Player {
            slot_id: id as u16,
            player_id: id,
            player_type: PlayerType::Human,
            race,
            team: id,
            name: name.to_string(),
            color: 0,
            custom_color: None,
        }
    }

    fn make_header(players: Vec<Player>, map_name: &str) -> Header {
        Header {
            engine: Engine::BroodWar,
            frame_count: 10000,
            start_time: 0,
            game_title: String::new(),
            map_width: 128,
            map_height: 128,
            game_speed: Speed::Fastest,
            game_type: GameType::Melee,
            host_name: String::new(),
            map_name: map_name.to_string(),
            players,
        }
    }

    // -- Matchup detection --

    #[test]
    fn test_matchup_tvz() {
        let m = detect_matchup(&Race::Terran, &Race::Zerg).unwrap();
        assert_eq!(m.code, "TvZ");
        assert!(!m.mirror);
    }

    #[test]
    fn test_matchup_zvt_normalizes() {
        let m = detect_matchup(&Race::Zerg, &Race::Terran).unwrap();
        assert_eq!(m.code, "TvZ");
    }

    #[test]
    fn test_matchup_pvt() {
        let m = detect_matchup(&Race::Protoss, &Race::Terran).unwrap();
        assert_eq!(m.code, "PvT");
    }

    #[test]
    fn test_matchup_pvz() {
        let m = detect_matchup(&Race::Protoss, &Race::Zerg).unwrap();
        assert_eq!(m.code, "PvZ");
    }

    #[test]
    fn test_matchup_mirror() {
        let m = detect_matchup(&Race::Terran, &Race::Terran).unwrap();
        assert_eq!(m.code, "TvT");
        assert!(m.mirror);
    }

    #[test]
    fn test_matchup_unknown_race() {
        assert!(detect_matchup(&Race::Unknown(5), &Race::Terran).is_none());
    }

    // -- Map name normalization --

    #[test]
    fn test_normalize_plain() {
        assert_eq!(normalize_map_name("Fighting Spirit"), "Fighting Spirit");
    }

    #[test]
    fn test_normalize_version_suffix() {
        assert_eq!(normalize_map_name("Fighting Spirit 1.3"), "Fighting Spirit");
    }

    #[test]
    fn test_normalize_v_prefix_version() {
        assert_eq!(
            normalize_map_name("Circuit Breaker v2.1"),
            "Circuit Breaker"
        );
    }

    #[test]
    fn test_normalize_player_count_prefix() {
        assert_eq!(normalize_map_name("(4)Fighting Spirit"), "Fighting Spirit");
    }

    #[test]
    fn test_normalize_both() {
        assert_eq!(
            normalize_map_name("(4)Fighting Spirit 1.3"),
            "Fighting Spirit"
        );
    }

    #[test]
    fn test_normalize_trailing_letter() {
        assert_eq!(normalize_map_name("Polypoid 1.3a"), "Polypoid");
    }

    #[test]
    fn test_normalize_whitespace() {
        assert_eq!(normalize_map_name("  Polypoid  "), "Polypoid");
    }

    #[test]
    fn test_normalize_no_false_positive() {
        // "SE" is not a version — it's "Special Edition".
        assert_eq!(normalize_map_name("Destination SE"), "Destination SE");
    }

    #[test]
    fn test_normalize_map_name_table() {
        let cases: &[(&str, &str)] = &[
            // League tags.
            ("| iCCup | Python 1.3", "Python"),
            ("| iCCup | Python", "Python"),
            ("|iCCup| Fighting Spirit", "Fighting Spirit"),
            ("[iCCup] Destination 1.1", "Destination"),
            ("Python (iCCup)", "Python"),
            ("iCCup Python 1.3", "Python"),
            ("| WGT | Circuit Breaker", "Circuit Breaker"),
            ("| iCCup | (4)Python 1.3", "Python"),
            // Attached version suffixes.
            ("Transistor1.2", "Transistor"),
            ("Polypoid1.65", "Polypoid"),
            ("Eclipse1.2a", "Eclipse"),
            ("Overwatchv1.1", "Overwatch"),
            // Separated version suffixes.
            ("Polypoid 1.65", "Polypoid"),
            ("Fighting Spirit 1.3", "Fighting Spirit"),
            ("Circuit Breaker v2.1", "Circuit Breaker"),
            ("Circuit Breaker ver 1.2", "Circuit Breaker"),
            ("Circuit Breaker ver1.2", "Circuit Breaker"),
            ("Match Point_1.3", "Match Point"),
            ("투혼 1.3", "투혼"),
            // StarCraft colour / control codes.
            ("\u{3}Fighting \u{6}Spirit\u{1}", "Fighting Spirit"),
            ("\u{4}| iCCup | \u{3}Python\u{1} 1.3", "Python"),
            // Must not change.
            ("Neo Sylphid2", "Neo Sylphid2"),
            ("Destination SE", "Destination SE"),
            ("Road War", "Road War"),
            ("Blue Storm", "Blue Storm"),
            ("8bit", "8bit"),
            ("1.3", "1.3"),
            ("iCCup", "iCCup"),
            ("", ""),
        ];
        for (raw, want) in cases {
            assert_eq!(&normalize_map_name(raw), want, "raw: {raw:?}");
        }
    }

    #[test]
    fn test_normalize_single_digit() {
        assert_eq!(normalize_map_name("Heartbreak Ridge 2"), "Heartbreak Ridge");
    }

    // -- Winner detection --

    #[test]
    fn test_winner_from_leave() {
        let header = make_header(
            vec![
                make_player(0, "Flash", Race::Terran),
                make_player(1, "Jaedong", Race::Zerg),
            ],
            "Test",
        );

        let commands = vec![GameCommand {
            frame: 5000,
            player_id: 1,
            command: Command::LeaveGame { reason: 1 },
        }];

        let result = detect_winner(&header, &commands);
        assert_eq!(
            result,
            GameResult::Winner {
                player_id: 0,
                player_name: "Flash".to_string(),
            }
        );
    }

    #[test]
    fn test_winner_no_leave() {
        let header = make_header(
            vec![
                make_player(0, "Flash", Race::Terran),
                make_player(1, "Jaedong", Race::Zerg),
            ],
            "Test",
        );

        let result = detect_winner(&header, &[]);
        assert_eq!(result, GameResult::Unknown);
    }

    #[test]
    fn test_winner_first_leave_loses() {
        let header = make_header(
            vec![
                make_player(0, "Flash", Race::Terran),
                make_player(1, "Jaedong", Race::Zerg),
            ],
            "Test",
        );

        // Both leave, but player 1 left first.
        let commands = vec![
            GameCommand {
                frame: 5000,
                player_id: 1,
                command: Command::LeaveGame { reason: 1 },
            },
            GameCommand {
                frame: 5010,
                player_id: 0,
                command: Command::LeaveGame { reason: 1 },
            },
        ];

        let result = detect_winner(&header, &commands);
        assert_eq!(
            result,
            GameResult::Winner {
                player_id: 0,
                player_name: "Flash".to_string(),
            }
        );
    }

    // -- Full metadata --

    #[test]
    fn test_extract_metadata() {
        let header = make_header(
            vec![
                make_player(0, "Flash", Race::Terran),
                make_player(1, "Jaedong", Race::Zerg),
            ],
            "(4)Fighting Spirit 1.3",
        );

        let commands = vec![GameCommand {
            frame: 5000,
            player_id: 1,
            command: Command::LeaveGame { reason: 1 },
        }];

        let meta = extract_metadata(&header, &commands);
        assert_eq!(meta.map_name_raw, "(4)Fighting Spirit 1.3");
        assert_eq!(meta.matchup.as_ref().unwrap().code, "TvZ");
        assert_eq!(meta.map_name, "Fighting Spirit");
        assert!(meta.is_1v1);
        assert_eq!(meta.player_count, 2);
        assert!(matches!(
            meta.result,
            GameResult::Winner { player_id: 0, .. }
        ));
    }
}
