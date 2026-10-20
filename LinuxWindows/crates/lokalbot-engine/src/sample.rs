//! LokalBot's sample meeting (`LokalBot/Resources/SampleMeeting`): a made-up
//! call between two synthetic voices, bundled so a new library can show a
//! cited answer before anything is recorded. It goes through the ordinary
//! import (`library_ops::import_sample`) and carries `sampleID`, so Ask,
//! search and agents see it while personal memory leaves it out.
//!
//! `Scripts/sample_meeting.py` builds the audio and its reference transcript,
//! and scores a transcript of it.

/// One recording of the sample, in one spoken language.
pub struct Sample {
    /// Stored as the meeting's `sampleID`: the script, then its language.
    pub id: &'static str,
    pub language: &'static str,
    /// The meeting's title, in the language spoken.
    pub title: &'static str,
    pub file_name: &'static str,
    pub audio: &'static [u8],
}

pub const ENGLISH: Sample = Sample {
    id: "orders-api/en",
    language: "en",
    title: "Sample: Orders API delivery window",
    file_name: "orders-api.en.m4a",
    audio: include_bytes!("../../../../LokalBot/Resources/SampleMeeting/orders-api.en.m4a"),
};

const SAMPLES: &[Sample] = &[ENGLISH];

/// The sample spoken in `language` (an interface language such as "de" or
/// "sr-Latn"), else the English one.
pub fn for_language(language: &str) -> &'static Sample {
    let base = language.split(['-', '_']).next().unwrap_or(language);
    SAMPLES
        .iter()
        .find(|s| s.language.eq_ignore_ascii_case(language))
        .or_else(|| {
            SAMPLES
                .iter()
                .find(|s| s.language.eq_ignore_ascii_case(base))
        })
        .unwrap_or(&ENGLISH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_bundled_script() {
        let script: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../LokalBot/Resources/SampleMeeting/script.en.json"
        ))
        .unwrap();
        assert_eq!(script["sampleID"], ENGLISH.id);
        assert_eq!(script["title"], ENGLISH.title);
        assert_eq!(script["audio"], ENGLISH.file_name);
    }

    #[test]
    fn falls_back_to_english() {
        assert_eq!(for_language("en").id, "orders-api/en");
        assert_eq!(for_language("de").id, "orders-api/en");
        assert_eq!(for_language("sr-Latn").id, "orders-api/en");
    }
}
