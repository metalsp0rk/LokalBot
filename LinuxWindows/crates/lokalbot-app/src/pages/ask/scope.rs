//! What a question may draw on: sources and a date scope (`AskSourceScope`,
//! `AskDateScope`).

use chrono::{DateTime, Datelike as _, Days, Local, NaiveDate, Utc};
use uuid::Uuid;

use crate::state::AskScope;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sources {
    pub meetings: bool,
    pub activity: bool,
    pub screen: bool,
}

impl Default for Sources {
    fn default() -> Self {
        Self {
            meetings: true,
            activity: true,
            screen: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Meetings,
    Activity,
    Screen,
}

impl Source {
    pub const ALL: [Source; 3] = [Source::Meetings, Source::Activity, Source::Screen];

    pub fn label(self) -> &'static str {
        match self {
            Source::Meetings => "Meetings",
            Source::Activity => "Activity",
            Source::Screen => "Screen",
        }
    }
}

impl Sources {
    pub fn contains(&self, source: Source) -> bool {
        match source {
            Source::Meetings => self.meetings,
            Source::Activity => self.activity,
            Source::Screen => self.screen,
        }
    }

    pub fn count(&self) -> usize {
        Source::ALL.iter().filter(|s| self.contains(**s)).count()
    }

    /// Toggles a source, keeping at least one selected.
    pub fn toggle(&mut self, source: Source) {
        if self.contains(source) && self.count() == 1 {
            return;
        }
        match source {
            Source::Meetings => self.meetings = !self.meetings,
            Source::Activity => self.activity = !self.activity,
            Source::Screen => self.screen = !self.screen,
        }
    }

    /// "All sources" or "Meetings, Screen".
    pub fn summary(&self) -> String {
        if *self == Sources::default() {
            return "All sources".into();
        }
        Source::ALL
            .iter()
            .filter(|s| self.contains(**s))
            .map(|s| s.label())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The calendar range a question or search is limited to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DateScope {
    #[default]
    AnyTime,
    Today,
    Yesterday,
    LastSevenDays,
    Day(NaiveDate),
}

impl DateScope {
    pub fn label(self) -> String {
        match self {
            DateScope::AnyTime => "Any time".into(),
            DateScope::Today => "Today".into(),
            DateScope::Yesterday => "Yesterday".into(),
            DateScope::LastSevenDays => "Last 7 days".into(),
            DateScope::Day(day) => {
                let today = Local::now().date_naive();
                if day == today {
                    "Today".into()
                } else if Some(day) == today.pred_opt() {
                    "Yesterday".into()
                } else if day.year() == today.year() {
                    day.format("%b %-d").to_string()
                } else {
                    day.format("%b %-d, %Y").to_string()
                }
            }
        }
    }

    /// `[since, until)` in UTC; `None` for any time.
    pub fn bounds(self) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        let today = Local::now().date_naive();
        let (first, last) = match self {
            DateScope::AnyTime => return None,
            DateScope::Today => (today, today),
            DateScope::Yesterday => {
                let day = today.pred_opt()?;
                (day, day)
            }
            DateScope::LastSevenDays => (today.checked_sub_days(Days::new(6))?, today),
            DateScope::Day(day) => (day, day),
        };
        let (since, _) = lokalbot_core::time::local_day_bounds(first);
        let (_, until) = lokalbot_core::time::local_day_bounds(last);
        Some((since, until))
    }
}

/// The request sent to the engine for one question.
pub fn ask_scope(sources: Sources, date: DateScope, meeting_ids: Option<&[Uuid]>) -> AskScope {
    let bounds = date.bounds();
    AskScope {
        meeting_ids: meeting_ids.map(<[Uuid]>::to_vec).unwrap_or_default(),
        include_meetings: sources.meetings,
        include_activity: sources.activity,
        include_screen: sources.screen,
        since: bounds.map(|(since, _)| since),
        until: bounds.map(|(_, until)| until),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_one_source() {
        let mut sources = Sources::default();
        assert_eq!(sources.summary(), "All sources");
        sources.toggle(Source::Activity);
        assert_eq!(sources.summary(), "Meetings, Screen");
        sources.toggle(Source::Meetings);
        sources.toggle(Source::Screen);
        assert_eq!(sources.summary(), "Screen");
    }

    #[test]
    fn seven_days_include_today() {
        let (since, until) = DateScope::LastSevenDays.bounds().unwrap();
        assert!(until > Utc::now());
        assert!((until - since).num_hours() >= 7 * 24 - 1);
        assert_eq!(DateScope::AnyTime.bounds(), None);
    }
}
