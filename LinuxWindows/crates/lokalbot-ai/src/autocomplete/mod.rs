//! Autocomplete (`LokalBot/Cotyping`): suggestions for the text field the
//! user is typing in, from the Autocomplete model on its own llama-server.
//!
//! This crate holds the prediction pipeline, the same for every model in the
//! catalog: a request from the field ([`prompt`]), one streamed completion
//! with token healing and the confidence gate ([`client`]), cleanup into
//! ghost text ([`normalize`], [`seam`]), and the rules for accepting,
//! typing through and topping up a suggestion ([`accept`], [`session`]),
//! with saved facts and learned examples when allowed ([`memory`],
//! [`learning`]). The engine's worker schedules it; the desktop integration
//! reads fields and draws and inserts the suggestions.

pub mod accept;
pub mod client;
pub mod confidence;
pub mod debounce;
pub mod emoji;
pub mod healing;
pub mod learning;
pub mod memory;
pub mod normalize;
pub mod prompt;
pub mod seam;
pub mod session;
pub mod stop;
pub mod surface;
mod text;
pub mod window;

use lokalbot_core::settings::AcceptGranularity;

pub use accept::{AcceptOptions, AcceptScope, AcceptancePlan};
pub use client::{Cancelled, Completer, Completion, Timings};
pub use normalize::{Normalized, Suppression};
pub use prompt::{Extras, Field, Personalization, Request, build_request};
pub use session::{Reconciled, Session, SuggestionKind};

/// The part of `suggestion` one Tab accepts: the next word or phrase,
/// with the spacing that belongs to it (`CotypingAcceptanceChunker`).
/// Punctuation after a word takes its own press, the macOS default.
pub fn accept_chunk(suggestion: &str, granularity: AcceptGranularity) -> String {
    match granularity {
        AcceptGranularity::Word => accept::next_word(suggestion, false).to_string(),
        AcceptGranularity::Phrase => accept::next_phrase(suggestion, false),
    }
}

/// The edit one Tab makes at the caret (`CotypingContinuationAcceptance`):
/// the chunk [`accept_chunk`] takes, the text to type for it, and what it
/// replaces before (an emoji's `:shortcode`) or after the caret (the rest of
/// a word the caret is inside of). `None` when nothing would be accepted.
pub fn plan_acceptance(
    suggestion: &str,
    granularity: AcceptGranularity,
    before: &str,
    after: &str,
) -> Option<AcceptancePlan> {
    let options = AcceptOptions {
        granularity,
        ..AcceptOptions::default()
    };
    accept::plan(suggestion, AcceptScope::Chunk, before, after, options)
}

/// [`plan_acceptance`] for the key that accepts the whole suggestion.
pub fn plan_full_acceptance(suggestion: &str, before: &str, after: &str) -> Option<AcceptancePlan> {
    accept::plan(
        suggestion,
        AcceptScope::Whole,
        before,
        after,
        AcceptOptions::default(),
    )
}

/// What to draw for an emoji suggestion ("🚀 :rocket:"), as macOS does; `None`
/// when `suggestion` is not the emoji for the `:shortcode` before the caret.
pub fn emoji_ghost(before: &str, suggestion: &str) -> Option<String> {
    emoji::ghost(before, suggestion)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_tab_takes_a_word_or_a_phrase() {
        assert_eq!(
            accept_chunk(" up on the timeline.", AcceptGranularity::Word),
            " up"
        );
        assert_eq!(
            accept_chunk(" yes, please", AcceptGranularity::Word),
            " yes"
        );
        assert_eq!(accept_chunk(",", AcceptGranularity::Word), ",");
        assert_eq!(
            accept_chunk(" up tomorrow. Then we plan.", AcceptGranularity::Phrase),
            " up tomorrow."
        );
        assert_eq!(accept_chunk("🚀", AcceptGranularity::Word), "🚀");
        assert_eq!(accept_chunk("", AcceptGranularity::Word), "");
    }

    #[test]
    fn plans_cover_spacing_mid_word_and_emoji() {
        let plan = plan_acceptance(" world", AcceptGranularity::Word, "Hello ", "").unwrap();
        assert_eq!(
            (plan.chunk.as_str(), plan.insertion.as_str()),
            (" world", "world")
        );
        let mid = plan_full_acceptance("ld peace", "Hello wor", "ld!").unwrap();
        assert_eq!(mid.forward_delete, 2);
        let emoji = plan_acceptance("🚀", AcceptGranularity::Word, "Ship it :roc", "").unwrap();
        assert_eq!(emoji.backward_delete, 4);
        assert_eq!(
            emoji_ghost("Ship it :roc", "🚀").as_deref(),
            Some("🚀 :rocket:")
        );
    }
}
