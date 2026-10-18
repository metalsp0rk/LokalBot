//! Projects: `projects/tracked.json` and `projects/summaries.json`
//! (`ProjectCuration.swift`, `ProjectStore.swift`, `ProjectLinks.swift`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ids::uuid_upper_serde;
use crate::model::outcomes::{ActionThread, Decision};
use crate::model::people::MeetingRef;
use crate::time::iso_seconds;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCuration {
    #[serde(default = "one")]
    pub version: u32,
    #[serde(default)]
    pub tracked: Vec<TrackedProject>,
    /// Normalized names the user dismissed as "Not a Project".
    #[serde(default)]
    pub hidden_names: Vec<String>,
}

fn one() -> u32 {
    1
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackedProject {
    #[serde(with = "uuid_upper_serde")]
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub other_names: Vec<String>,
    #[serde(with = "iso_seconds")]
    pub created_at: DateTime<Utc>,
}

/// One entry of `projects/summaries.json`: the "Where It Stands" note.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    #[serde(rename = "projectID")]
    pub project_id: String,
    pub overview: String,
    #[serde(default)]
    pub points: Vec<ProjectPoint>,
    #[serde(default)]
    pub open_questions: Vec<String>,
    #[serde(default, rename = "meetingIDs")]
    pub meeting_ids: Vec<String>,
    #[serde(default)]
    pub evidence_fingerprint: String,
    #[serde(with = "iso_seconds")]
    pub generated_at: DateTime<Utc>,
    #[serde(default)]
    pub engine_name: String,
    /// "on this PC" or the remote host that wrote it.
    #[serde(default)]
    pub location: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPoint {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "meetingID")]
    pub meeting_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meeting_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meeting_date: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectKind {
    Tracked,
    /// A capitalized term seen in three or more meetings.
    Suggested,
}

/// A project joined with its meetings, actions and decisions.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectProfile {
    /// `tracked:<UUID>` or `topic:<normalized name>`.
    pub id: String,
    pub kind: ProjectKind,
    pub name: String,
    pub other_names: Vec<String>,
    pub meetings: Vec<MeetingRef>,
    pub my_actions: Vec<ActionThread>,
    pub others_actions: Vec<ActionThread>,
    pub decisions: Vec<(MeetingRef, Decision)>,
    pub people: Vec<String>,
    pub week_seconds: f64,
    pub summary: Option<ProjectSummary>,
}
