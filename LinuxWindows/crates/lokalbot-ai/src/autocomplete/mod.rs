//! Autocomplete (`LokalBot/Cotyping`): suggestions for the text field the
//! user is typing in, from the Autocomplete model.

use lokalbot_core::settings::AcceptGranularity;

/// The part of `suggestion` one Tab accepts: the next word or phrase,
/// with the spacing that belongs to it (`CotypingAcceptanceChunker`).
pub fn accept_chunk(suggestion: &str, granularity: AcceptGranularity) -> String {
    let _ = granularity;
    let leading = suggestion.len() - suggestion.trim_start().len();
    let word_end = suggestion[leading..]
        .find(char::is_whitespace)
        .map_or(suggestion.len(), |end| leading + end);
    suggestion[..word_end].to_string()
}
