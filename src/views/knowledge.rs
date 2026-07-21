//! Presentation-safe technical-knowledge browser models.

use loco_rs::{
    controller::views::{engines::TeraView, ViewRenderer},
    Result,
};
use serde::{Deserialize, Serialize};

use crate::{
    controllers::browser::forms::FormState,
    domain::Page,
    models::{
        attachment::AttachmentMetadata, intervention::Intervention, technical_note::TechnicalNote,
        vehicle::Vehicle,
    },
};

use super::layout::AuthenticatedLayout;

const LIST_PAGE: &str = "pages/knowledge.html";
const LIST_FRAGMENT: &str = "fragments/knowledge_list.html";
const FORM_PAGE: &str = "pages/knowledge_form.html";
const FORM_FRAGMENT: &str = "fragments/knowledge_form.html";
const DETAIL_PAGE: &str = "pages/knowledge_detail.html";
const DETAIL_FRAGMENT: &str = "fragments/knowledge_detail.html";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct KnowledgeFilterValues {
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub tags: String,
    #[serde(default)]
    pub make: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub engine: String,
    #[serde(default)]
    pub archived: String,
    #[serde(default)]
    pub cursor: String,
}

impl KnowledgeFilterValues {
    #[must_use]
    pub fn has_filters(&self) -> bool {
        !self.q.is_empty()
            || !self.tags.is_empty()
            || !self.make.is_empty()
            || !self.model.is_empty()
            || !self.engine.is_empty()
    }
}

#[derive(Debug, Serialize)]
struct KnowledgeListItem {
    title: String,
    excerpt: String,
    tags: Vec<String>,
    context: Vec<String>,
    has_source: bool,
    updated_at: String,
    archived: bool,
    href: String,
}

#[derive(Debug, Serialize)]
pub struct KnowledgeListPage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: &'static str,
    filters: KnowledgeFilterValues,
    items: Vec<KnowledgeListItem>,
    next_href: Option<String>,
    filter_error: Option<String>,
    has_filters: bool,
    archive_label: &'static str,
}

impl<'page> KnowledgeListPage<'page> {
    #[must_use]
    pub fn new(
        layout: AuthenticatedLayout<'page>,
        filters: KnowledgeFilterValues,
        page: Page<TechnicalNote>,
        next_href: Option<String>,
        filter_error: Option<String>,
    ) -> Self {
        let archive_label = match filters.archived.as_str() {
            "archived" => "archived",
            "all" => "active or archived",
            _ => "active",
        };
        let has_filters = filters.has_filters();
        Self {
            layout,
            title: "Technical knowledge · Pipauto",
            filters,
            items: page.items.into_iter().map(list_item).collect(),
            next_href,
            filter_error,
            has_filters,
            archive_label,
        }
    }

    pub fn render_page(&self, engine: &TeraView) -> Result<String> {
        engine.render(LIST_PAGE, self)
    }

    pub fn render_content(&self, engine: &TeraView) -> Result<String> {
        engine.render(LIST_FRAGMENT, self)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct KnowledgeFormValues {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub tags: String,
    #[serde(default)]
    pub make: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub engine: String,
    #[serde(default)]
    pub vehicle_id: String,
    #[serde(default)]
    pub source_intervention_id: String,
    #[serde(default)]
    pub resolution: String,
}

impl From<&TechnicalNote> for KnowledgeFormValues {
    fn from(note: &TechnicalNote) -> Self {
        Self {
            title: note.title.clone(),
            body: note.body.clone(),
            tags: note.tags.join("\n"),
            make: context_display(note.make.as_ref()),
            model: context_display(note.model.as_ref()),
            engine: context_display(note.engine.as_ref()),
            vehicle_id: optional_id(note.vehicle_id.as_ref().map(|id| id.as_str())),
            source_intervention_id: optional_id(
                note.source_intervention_id.as_ref().map(|id| id.as_str()),
            ),
            resolution: String::new(),
        }
    }
}

#[derive(Debug, Serialize)]
struct SelectOption {
    id: String,
    label: String,
    selected: bool,
}

#[derive(Debug, Serialize)]
struct AttachmentItem {
    display_name: String,
    media_type: String,
    byte_size: u64,
    caption: Option<String>,
    open_href: String,
    download_href: String,
    edit_href: String,
    delete_action: String,
}

#[derive(Debug, Serialize)]
pub struct KnowledgeFormPage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: &'static str,
    heading: &'static str,
    action: String,
    submit_label: &'static str,
    cancel_href: String,
    form: FormState<KnowledgeFormValues>,
    tags: Vec<String>,
    vehicles: Vec<SelectOption>,
    interventions: Vec<SelectOption>,
    conflict: Option<String>,
    has_errors: bool,
    reload_href: String,
}

impl<'page> KnowledgeFormPage<'page> {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        layout: AuthenticatedLayout<'page>,
        editing: bool,
        action: String,
        cancel_href: String,
        form: FormState<KnowledgeFormValues>,
        vehicles: Vec<Vehicle>,
        interventions: Vec<(Intervention, Vehicle)>,
        conflict: Option<String>,
    ) -> Self {
        let selected_vehicle = form.values.vehicle_id.clone();
        let selected_source = form.values.source_intervention_id.clone();
        let tags = form
            .values
            .tags
            .lines()
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(str::to_owned)
            .collect();
        let has_errors = !form.errors.is_empty();
        let reload_href = if editing {
            cancel_href.clone()
        } else {
            "/knowledge/new".to_owned()
        };
        Self {
            layout,
            title: if editing {
                "Edit technical note · Pipauto"
            } else {
                "New technical note · Pipauto"
            },
            heading: if editing {
                "Edit technical note"
            } else {
                "New technical note"
            },
            action,
            submit_label: if editing {
                "Save changes"
            } else {
                "Save technical note"
            },
            cancel_href,
            tags,
            vehicles: vehicles
                .into_iter()
                .map(|vehicle| SelectOption {
                    selected: selected_vehicle == vehicle.id.as_str(),
                    id: vehicle.id.as_str().to_owned(),
                    label: vehicle_label(&vehicle),
                })
                .collect(),
            interventions: interventions
                .into_iter()
                .map(|(intervention, _vehicle)| SelectOption {
                    selected: selected_source == intervention.id.as_str(),
                    id: intervention.id.as_str().to_owned(),
                    label: format!(
                        "{} · {} {}",
                        intervention.service_date.format("%d %b %Y"),
                        intervention.identity_snapshot.vehicle_make,
                        intervention.identity_snapshot.vehicle_model,
                    ),
                })
                .collect(),
            conflict,
            has_errors,
            reload_href,
            form,
        }
    }

    pub fn render_page(&self, engine: &TeraView) -> Result<String> {
        engine.render(FORM_PAGE, self)
    }

    pub fn render_form(&self, engine: &TeraView) -> Result<String> {
        engine.render(FORM_FRAGMENT, self)
    }
}

#[derive(Debug, Serialize)]
pub struct KnowledgeDetailPage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: &'static str,
    id: String,
    note_title: String,
    body: String,
    tags: Vec<String>,
    context: Vec<String>,
    archived: bool,
    vehicle: Option<SelectOption>,
    source: Option<SelectOption>,
    created_at: String,
    updated_at: String,
    archived_at: Option<String>,
    attachments: Vec<AttachmentItem>,
}

impl<'page> KnowledgeDetailPage<'page> {
    #[must_use]
    pub fn new(
        layout: AuthenticatedLayout<'page>,
        note: TechnicalNote,
        vehicle: Option<Vehicle>,
        source: Option<Intervention>,
        attachments: Vec<AttachmentMetadata>,
    ) -> Self {
        let id = note.id.as_str().to_owned();
        let context = note_context(&note);
        let archived = note.is_archived();
        Self {
            layout,
            title: "Technical note · Pipauto",
            id: id.clone(),
            note_title: note.title,
            body: note.body,
            tags: note.tags,
            context,
            archived,
            vehicle: vehicle.map(|vehicle| SelectOption {
                id: format!("/vehicles/{}", vehicle.id.as_str()),
                label: vehicle_label(&vehicle),
                selected: false,
            }),
            source: source.map(|intervention| SelectOption {
                id: format!("/interventions/{}", intervention.id.as_str()),
                label: intervention.service_date.format("%d %b %Y").to_string(),
                selected: false,
            }),
            created_at: timestamp(note.created_at),
            updated_at: timestamp(note.updated_at),
            archived_at: note.archived_at.map(timestamp),
            attachments: attachments
                .into_iter()
                .map(|attachment| {
                    let attachment_id = attachment.id.as_str().to_owned();
                    AttachmentItem {
                        display_name: attachment.display_name,
                        media_type: attachment.media_type.as_str().to_owned(),
                        byte_size: attachment.byte_size,
                        caption: attachment.caption,
                        open_href: format!("/attachments/{attachment_id}/content"),
                        download_href: format!("/attachments/{attachment_id}/download"),
                        edit_href: format!("/knowledge/{id}/attachments/{attachment_id}/edit"),
                        delete_action: format!(
                            "/knowledge/{id}/attachments/{attachment_id}/delete"
                        ),
                    }
                })
                .collect(),
        }
    }

    pub fn render_page(&self, engine: &TeraView) -> Result<String> {
        engine.render(DETAIL_PAGE, self)
    }

    pub fn render_content(&self, engine: &TeraView) -> Result<String> {
        engine.render(DETAIL_FRAGMENT, self)
    }
}

fn list_item(note: TechnicalNote) -> KnowledgeListItem {
    let excerpt = note.body.chars().take(220).collect::<String>();
    let context = note_context(&note);
    let archived = note.is_archived();
    KnowledgeListItem {
        title: note.title,
        excerpt: if note.body.chars().count() > 220 {
            format!("{excerpt}…")
        } else {
            excerpt
        },
        tags: note.tags,
        context,
        has_source: note.source_intervention_id.is_some(),
        updated_at: note.updated_at.format("%d %b %Y").to_string(),
        archived,
        href: format!("/knowledge/{}", note.id.as_str()),
    }
}

fn note_context(note: &TechnicalNote) -> Vec<String> {
    [
        note.make.as_ref(),
        note.model.as_ref(),
        note.engine.as_ref(),
    ]
    .into_iter()
    .flatten()
    .map(|value| value.display.clone())
    .collect()
}

fn context_display(value: Option<&crate::models::technical_note::TechnicalNoteContext>) -> String {
    value.map_or_else(String::new, |value| value.display.clone())
}

fn optional_id(value: Option<&str>) -> String {
    value.map_or_else(String::new, str::to_owned)
}

fn vehicle_label(vehicle: &Vehicle) -> String {
    format!(
        "{} · {} {}",
        vehicle.registration.as_deref().unwrap_or("No registration"),
        vehicle.make,
        vehicle.model
    )
}

fn timestamp(value: chrono::DateTime<chrono::Utc>) -> String {
    value.format("%d %b %Y, %H:%M UTC").to_string()
}
