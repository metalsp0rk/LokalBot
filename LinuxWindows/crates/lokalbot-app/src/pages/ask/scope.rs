//! The date scope of a question or search (`AskDateScope`). The Mac's
//! source picker (`AskSourceScope`) is not offered: Linux and Windows have
//! no screen memory, and answers draw on meetings only (activity is not a
//! source in `lokalbot_ai::ask`).

use chrono::{DateTime, Datelike as _, Days, Local, NaiveDate, Utc};
use uuid::Uuid;

use crate::state::AskScope;

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

/// The request sent to the engine for one question: meetings only.
pub fn ask_scope(date: DateScope, meeting_ids: Option<&[Uuid]>) -> AskScope {
    let bounds = date.bounds();
    AskScope {
        meeting_ids: meeting_ids.map(<[Uuid]>::to_vec).unwrap_or_default(),
        include_meetings: true,
        include_activity: false,
        include_screen: false,
        since: bounds.map(|(since, _)| since),
        until: bounds.map(|(_, until)| until),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_meetings_only() {
        let scope = ask_scope(DateScope::Today, None);
        assert!(scope.include_meetings);
        assert!(!scope.include_activity && !scope.include_screen);
        assert!(scope.since.is_some() && scope.until.is_some());
    }

    #[test]
    fn seven_days_include_today() {
        let (since, until) = DateScope::LastSevenDays.bounds().unwrap();
        assert!(until > Utc::now());
        assert!((until - since).num_hours() >= 7 * 24 - 1);
        assert_eq!(DateScope::AnyTime.bounds(), None);
    }
}
