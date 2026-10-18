//! Keyword results for Ask and Quick Recall, grouped by meeting
//! (`RecallSearch.groups`). Runs on a background thread.
//!
//! This scans meeting files directly; it is replaced by the library's FTS
//! search once that lands in `lokalbot-core`.

use chrono::{DateTime, Utc};
use lokalbot_core::{Library, Meeting};
use uuid::Uuid;

/// Where in a meeting a keyword matched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitKind {
    Title,
    Summary,
    Notes,
    Outcome,
    Segment,
}

impl HitKind {
    fn rank(self) -> u8 {
        match self {
            HitKind::Title => 0,
            HitKind::Summary => 1,
            HitKind::Outcome => 2,
            HitKind::Notes => 3,
            HitKind::Segment => 4,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub kind: HitKind,
    /// Transcript time for segment hits.
    pub start: f64,
    pub speaker: String,
    /// Snippet with «matched» words.
    pub snippet: String,
}

impl Hit {
    /// "Title", "Summary", "Your notes", "Outcome" or "▶ 00:12:04 · Maya".
    pub fn kind_label(&self) -> String {
        match self.kind {
            HitKind::Title => "Title".into(),
            HitKind::Summary => "Summary".into(),
            HitKind::Notes => "Your notes".into(),
            HitKind::Outcome => "Outcome".into(),
            HitKind::Segment if self.speaker.is_empty() => {
                format!("▶ {}", lokalbot_core::time::stamp(self.start))
            }
            HitKind::Segment => format!(
                "▶ {} · {}",
                lokalbot_core::time::stamp(self.start),
                self.speaker
            ),
        }
    }

    /// Quick Recall's meta kind: "Transcript", "Your notes", "Summary".
    pub fn recall_kind(&self) -> &'static str {
        match self.kind {
            HitKind::Segment => "Transcript",
            HitKind::Notes => "Your notes",
            HitKind::Title => "Title",
            HitKind::Summary => "Summary",
            HitKind::Outcome => "Outcome",
        }
    }
}

/// All matches in one meeting; `hits[0]` is the primary row.
#[derive(Clone, Debug, PartialEq)]
pub struct MeetingGroup {
    pub meeting: Meeting,
    pub hits: Vec<Hit>,
}

/// Result-type filter (`AskFacet`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Facet {
    #[default]
    All,
    Transcripts,
    Summaries,
    Notes,
    Outcomes,
    Screen,
}

impl Facet {
    pub const ALL: [Facet; 6] = [
        Facet::All,
        Facet::Transcripts,
        Facet::Summaries,
        Facet::Notes,
        Facet::Outcomes,
        Facet::Screen,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Facet::All => "All",
            Facet::Transcripts => "Transcripts",
            Facet::Summaries => "Summaries",
            Facet::Notes => "Notes",
            Facet::Outcomes => "Outcomes",
            Facet::Screen => "Screen",
        }
    }

    fn allows(self, kind: HitKind) -> bool {
        match self {
            Facet::All => true,
            Facet::Transcripts => kind == HitKind::Segment,
            Facet::Summaries => kind == HitKind::Summary || kind == HitKind::Title,
            Facet::Notes => kind == HitKind::Notes,
            Facet::Outcomes => kind == HitKind::Outcome,
            Facet::Screen => false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Query {
    pub text: String,
    pub facet: Facet,
    pub meeting_ids: Option<Vec<Uuid>>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    /// Maximum number of meeting groups.
    pub limit: usize,
}

/// Matching meetings, best first, and whether more matched than `limit`.
pub fn run(library: &Library, meetings: &[Meeting], query: &Query) -> (Vec<MeetingGroup>, bool) {
    let terms = terms(&query.text);
    if terms.is_empty() || query.facet == Facet::Screen {
        return (Vec::new(), false);
    }
    let mut groups = Vec::new();
    for meeting in meetings {
        if query
            .meeting_ids
            .as_ref()
            .is_some_and(|ids| !ids.contains(&meeting.id))
        {
            continue;
        }
        if query.since.is_some_and(|since| meeting.started_at < since)
            || query.until.is_some_and(|until| meeting.started_at >= until)
        {
            continue;
        }
        let mut hits = Vec::new();
        let mut consider = |kind: HitKind, text: &str, start: f64, speaker: &str| {
            if query.facet.allows(kind)
                && let Some(snippet) = snippet(text, &terms)
            {
                hits.push(Hit {
                    kind,
                    start,
                    speaker: speaker.to_string(),
                    snippet,
                });
            }
        };
        consider(HitKind::Title, &meeting.display_title(), 0.0, "");
        if let Ok(Some(summary)) = library.summary(meeting) {
            consider(HitKind::Summary, &strip_markdown(&summary), 0.0, "");
        }
        if let Ok(notes) = library.notes(meeting) {
            consider(HitKind::Notes, &notes, 0.0, "");
        }
        if let Ok(Some(outcomes)) = library.outcomes(meeting) {
            for item in &outcomes.action_items {
                let start = item.citations.first().map(|c| c.start).unwrap_or(0.0);
                consider(HitKind::Outcome, &item.text, start, "");
            }
            for decision in &outcomes.decision_records {
                let start = decision.citations.first().map(|c| c.start).unwrap_or(0.0);
                consider(HitKind::Outcome, &decision.text, start, "");
            }
        }
        if let Ok(Some(transcript)) = library.transcript(meeting) {
            for segment in &transcript.segments {
                let speaker = transcript
                    .speaker_aliases
                    .get(&segment.speaker)
                    .cloned()
                    .unwrap_or_else(|| speaker_label(&segment.speaker));
                consider(HitKind::Segment, &segment.text, segment.start, &speaker);
            }
        }
        if hits.is_empty() {
            continue;
        }
        hits.sort_by_key(|hit| hit.kind.rank());
        groups.push(MeetingGroup {
            meeting: meeting.clone(),
            hits,
        });
    }
    groups.sort_by(|a, b| {
        let rank = |g: &MeetingGroup| g.hits[0].kind.rank();
        rank(a)
            .cmp(&rank(b))
            .then(b.hits.len().cmp(&a.hits.len()))
            .then(b.meeting.started_at.cmp(&a.meeting.started_at))
    });
    let truncated = query.limit > 0 && groups.len() > query.limit;
    if truncated {
        groups.truncate(query.limit);
    }
    (groups, truncated)
}

/// Lowercased words of at least two characters.
pub fn terms(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-')
        .map(|t| {
            t.trim_matches(|c: char| c == '\'' || c == '-')
                .to_lowercase()
        })
        .filter(|t| t.chars().count() >= 2)
        .collect()
}

/// About 160 characters around the first match, every term wrapped in «».
/// `None` unless every term occurs.
pub fn snippet(text: &str, terms: &[String]) -> Option<String> {
    let lower = text.to_lowercase();
    // Lowercasing can change byte lengths for some scripts; fall back to no highlighting there.
    let aligned = lower.len() == text.len();
    if terms.iter().any(|term| !lower.contains(term.as_str())) {
        return None;
    }
    let first = terms
        .iter()
        .filter_map(|term| lower.find(term.as_str()))
        .min()
        .unwrap_or(0);
    let mut start = first.saturating_sub(60);
    while start > 0 && !text.is_char_boundary(start) {
        start -= 1;
    }
    if start > 0 {
        start = text[start..first]
            .find(' ')
            .map(|space| start + space + 1)
            .unwrap_or(start);
    }
    let mut end = (first + 160).min(text.len());
    while end < text.len() && !text.is_char_boundary(end) {
        end += 1;
    }
    let window = &text[start..end];
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    if aligned {
        let window_lower = &lower[start..end];
        let mut cursor = 0;
        while cursor < window.len() {
            let next = terms
                .iter()
                .filter_map(|term| {
                    window_lower[cursor..]
                        .find(term.as_str())
                        .map(|at| (cursor + at, term.len()))
                })
                .min_by_key(|(at, _)| *at);
            match next {
                Some((at, len))
                    if window.is_char_boundary(at) && window.is_char_boundary(at + len) =>
                {
                    out.push_str(&window[cursor..at]);
                    out.push('«');
                    out.push_str(&window[at..at + len]);
                    out.push('»');
                    cursor = at + len;
                }
                _ => {
                    out.push_str(&window[cursor..]);
                    break;
                }
            }
        }
    } else {
        out.push_str(window);
    }
    if end < text.len() {
        out.push('…');
    }
    Some(out.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn strip_markdown(text: &str) -> String {
    text.lines()
        .map(|line| {
            let line = line.trim_start_matches(['#', '-', '*', '>', ' ']);
            let line = ["[ ] ", "[x] ", "[X] "]
                .iter()
                .fold(line, |l, box_| l.strip_prefix(box_).unwrap_or(l));
            line.replace("**", "")
        })
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `SpeakerDisplayName.label`: "Them" reads as "Other speaker", "Them 2" as "Speaker 2".
pub fn speaker_label(name: &str) -> String {
    let trimmed = name.trim();
    let lower = trimmed.to_lowercase();
    if lower == "them" {
        return "Other speaker".into();
    }
    if let Some(number) = lower.strip_prefix("them ")
        && !number.is_empty()
        && number.bytes().all(|b| b.is_ascii_digit())
    {
        return format!("Speaker {number}");
    }
    trimmed.to_string()
}

/// Questions go to the assistant on Return (`AskIntent.isQuestion`).
pub fn is_question(query: &str) -> bool {
    const STARTS: &[&str] = &[
        "what",
        "why",
        "how",
        "when",
        "where",
        "who",
        "whom",
        "whose",
        "which",
        "did",
        "do",
        "does",
        "is",
        "are",
        "was",
        "were",
        "can",
        "could",
        "should",
        "would",
        "will",
        "has",
        "have",
        "had",
        "summarize",
        "summarise",
        "explain",
        "list",
        "tell",
        "compare",
        "remind",
        "draft",
    ];
    let words: Vec<&str> = query.split_whitespace().collect();
    let Some(first) = words.first() else {
        return false;
    };
    if query.trim().ends_with('?') {
        return words.len() >= 2;
    }
    let lead = first
        .trim_matches(|c: char| c.is_ascii_punctuation())
        .to_lowercase();
    words.len() >= 3 && STARTS.contains(&lead.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippets_mark_every_term() {
        let terms = terms("Redis cluster");
        let text = "We adopt Redis for caching; cluster mode waits for the failover benchmark.";
        assert_eq!(
            snippet(text, &terms).as_deref(),
            Some("We adopt «Redis» for caching; «cluster» mode waits for the failover benchmark.")
        );
        assert_eq!(snippet("Only Redis here", &terms), None);
    }

    #[test]
    fn long_text_is_windowed() {
        let text = format!(
            "{} captions are easy {}",
            "word ".repeat(40),
            "tail ".repeat(60)
        );
        let snippet = snippet(&text, &terms("captions")).unwrap();
        assert!(snippet.starts_with('…'));
        assert!(snippet.ends_with('…'));
        assert!(snippet.contains("«captions»"));
    }

    #[test]
    fn question_detection_matches_the_mac() {
        assert!(is_question("What did we decide about Redis?"));
        assert!(is_question("redis status?"));
        assert!(is_question("list my recent meetings"));
        assert!(!is_question("redis cluster"));
        assert!(!is_question("what?"));
    }
}
