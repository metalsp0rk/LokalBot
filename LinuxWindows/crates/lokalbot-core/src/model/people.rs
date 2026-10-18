//! People: `people/curation.json` plus profiles derived from meetings
//! (`PeopleDirectory.swift`, `PeopleCuration.swift`).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::outcomes::{ActionThread, Decision};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeopleCuration {
    #[serde(default = "one")]
    pub version: u32,
    #[serde(default)]
    pub names: BTreeMap<String, String>,
    #[serde(default)]
    pub merges: Vec<PersonMerge>,
    #[serde(default)]
    pub hidden: Vec<HiddenPerson>,
}

fn one() -> u32 {
    1
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonMerge {
    #[serde(rename = "sourceID")]
    pub source_id: String,
    #[serde(rename = "targetID")]
    pub target_id: String,
    pub source_name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HiddenPerson {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeetingRef {
    pub id: Uuid,
    pub title: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub participant_count: usize,
}

/// A person assembled from speaker names, calendar attendees and action owners.
#[derive(Clone, Debug, PartialEq)]
pub struct PersonProfile {
    /// `person-%016llx` of the join key.
    pub id: String,
    pub name: String,
    pub other_names: Vec<String>,
    pub meetings: Vec<MeetingRef>,
    /// Open work the person owes the user.
    pub their_actions: Vec<ActionThread>,
    /// Open work the user owes the person.
    pub my_actions: Vec<ActionThread>,
    pub decisions: Vec<(MeetingRef, Decision)>,
    pub recent_notes: Vec<String>,
    pub projects: Vec<String>,
}

impl PersonProfile {
    pub fn last_met(&self) -> Option<DateTime<Utc>> {
        self.meetings.iter().map(|m| m.started_at).max()
    }

    pub fn first_name(&self) -> &str {
        self.name.split_whitespace().next().unwrap_or(&self.name)
    }

    pub fn initials(&self) -> String {
        self.name
            .split_whitespace()
            .filter_map(|w| w.chars().next())
            .take(2)
            .collect::<String>()
            .to_uppercase()
    }
}
