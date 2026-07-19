//! Searchable, database-independent workshop knowledge.

use crate::domain::{normalize_search_text, InterventionId, VehicleId};

pub const TITLE_MAX_CHARS: usize = 200;
pub const BODY_MAX_CHARS: usize = 50_000;
pub const TAG_MAX_CHARS: usize = 64;
pub const TAG_MAX_COUNT: usize = 20;
pub const MAKE_MAX_CHARS: usize = 80;
pub const MODEL_MAX_CHARS: usize = 80;
pub const ENGINE_MAX_CHARS: usize = 160;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TechnicalNoteContext {
    pub display: String,
    pub normalized: String,
}

impl TechnicalNoteContext {
    fn optional(
        value: Option<String>,
        maximum: usize,
    ) -> Result<Option<Self>, TechnicalNoteModelError> {
        optional_text(value, maximum).map(|value| {
            value.map(|display| Self {
                normalized: normalize_search_text(&display),
                display,
            })
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewTechnicalNote {
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub vehicle_id: Option<VehicleId>,
    pub source_intervention_id: Option<InterventionId>,
    pub make: Option<TechnicalNoteContext>,
    pub model: Option<TechnicalNoteContext>,
    pub engine: Option<TechnicalNoteContext>,
}

impl NewTechnicalNote {
    /// Validate human-authored knowledge and derive deterministic exact-search values.
    ///
    /// Tags are normalized, sorted, and deduplicated because they are structured filters rather
    /// than display values. Optional vehicle and intervention references are independent here;
    /// their existence and cross-record consistency belong to the later service layer.
    ///
    /// # Errors
    ///
    /// Rejects blank or oversized text and tag lists above [`TAG_MAX_COUNT`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        title: String,
        body: String,
        tags: Vec<String>,
        vehicle_id: Option<VehicleId>,
        source_intervention_id: Option<InterventionId>,
        make: Option<String>,
        model: Option<String>,
        engine: Option<String>,
    ) -> Result<Self, TechnicalNoteModelError> {
        let title = required_text(title, TITLE_MAX_CHARS)?;
        let body = required_text(body, BODY_MAX_CHARS)?;
        if tags.len() > TAG_MAX_COUNT {
            return Err(TechnicalNoteModelError::TooManyTags);
        }
        let mut tags = tags
            .into_iter()
            .map(|tag| {
                let tag = required_text(tag, TAG_MAX_CHARS)?;
                Ok(normalize_search_text(&tag))
            })
            .collect::<Result<Vec<_>, TechnicalNoteModelError>>()?;
        tags.sort_unstable();
        tags.dedup();

        Ok(Self {
            title,
            body,
            tags,
            vehicle_id,
            source_intervention_id,
            make: TechnicalNoteContext::optional(make, MAKE_MAX_CHARS)?,
            model: TechnicalNoteContext::optional(model, MODEL_MAX_CHARS)?,
            engine: TechnicalNoteContext::optional(engine, ENGINE_MAX_CHARS)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum TechnicalNoteModelError {
    #[error("required technical-note text is blank")]
    Required,
    #[error("technical-note text exceeds its maximum length")]
    TooLong,
    #[error("technical note has too many tags")]
    TooManyTags,
}

fn required_text(value: String, maximum: usize) -> Result<String, TechnicalNoteModelError> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Err(TechnicalNoteModelError::Required);
    }
    if value.chars().count() > maximum {
        return Err(TechnicalNoteModelError::TooLong);
    }
    Ok(value)
}

fn optional_text(
    value: Option<String>,
    maximum: usize,
) -> Result<Option<String>, TechnicalNoteModelError> {
    value
        .map(|value| {
            let value = value.trim().to_owned();
            if value.is_empty() {
                return Ok(None);
            }
            if value.chars().count() > maximum {
                return Err(TechnicalNoteModelError::TooLong);
            }
            Ok(Some(value))
        })
        .transpose()
        .map(Option::flatten)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn technical_note_normalizes_tags_and_search_context() {
        let note = NewTechnicalNote::new(
            "  Water pump replacement  ".into(),
            "  Lock the crankshaft before removing the pulley.  ".into(),
            vec![" Cooling System ".into(), "VW".into(), "vw".into()],
            Some(VehicleId::parse("golf").expect("valid vehicle id")),
            Some(InterventionId::parse("job-42").expect("valid intervention id")),
            Some(" Volkswagen ".into()),
            Some(" Golf  GTE ".into()),
            Some(" 1.4 TSI  Hybrid ".into()),
        )
        .expect("valid technical note");

        assert_eq!(note.title, "Water pump replacement");
        assert_eq!(
            note.tags,
            vec!["cooling system".to_owned(), "vw".to_owned()]
        );
        assert_eq!(
            note.make.as_ref().map(|value| value.display.as_str()),
            Some("Volkswagen")
        );
        assert_eq!(
            note.model.as_ref().map(|value| value.normalized.as_str()),
            Some("golf gte")
        );
        assert_eq!(
            note.engine.as_ref().map(|value| value.normalized.as_str()),
            Some("1.4 tsi hybrid")
        );
    }

    #[test]
    fn technical_note_enforces_required_text_and_tag_limits() {
        assert_eq!(
            NewTechnicalNote::new(
                " ".into(),
                "Body".into(),
                vec![],
                None,
                None,
                None,
                None,
                None,
            ),
            Err(TechnicalNoteModelError::Required)
        );
        assert_eq!(
            NewTechnicalNote::new(
                "Title".into(),
                "Body".into(),
                (0..=TAG_MAX_COUNT)
                    .map(|index| format!("tag-{index}"))
                    .collect(),
                None,
                None,
                None,
                None,
                None,
            ),
            Err(TechnicalNoteModelError::TooManyTags)
        );
    }
}
