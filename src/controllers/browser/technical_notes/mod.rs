//! Server-rendered technical-knowledge search, authoring, detail, and lifecycle workflows.

use axum::{
    extract::{DefaultBodyLimit, Query},
    http::StatusCode,
    response::Response,
};
use loco_rs::{
    controller::{
        extractor::shared_store::SharedStore, views::engines::TeraView, views::ViewEngine, Routes,
    },
    prelude::{get, post, Path},
    Result,
};
use serde::Deserialize;

use crate::{
    auth::csrf::AuthenticatedAttachmentMultipart,
    controllers::browser::{
        context::BrowserRequestContext,
        forms::{body_limit, optional_text, AuthenticatedForm, FormState},
        responses,
    },
    domain::{
        normalize_search_text, AttachmentId, InterventionId, OpaqueCursor, Page, PageLimit,
        PageRequest, TechnicalNoteId, ValidationCode, ValidationError, ValidationErrors, VehicleId,
    },
    models::{
        attachment::{
            AttachmentMetadata, AttachmentModel as AttachmentService, AttachmentOwner,
            UploadAttachment, WriteAttachmentMetadata, CAPTION_MAX_CHARS, DISPLAY_NAME_MAX_CHARS,
        },
        customer::ArchiveFilter,
        intervention::{
            Intervention, InterventionFilter, InterventionModel as InterventionService,
        },
        technical_note::{
            validate_write, NewTechnicalNote, TechnicalNoteFilter,
            TechnicalNoteModel as TechnicalNoteService, BODY_MAX_CHARS, ENGINE_MAX_CHARS,
            MAKE_MAX_CHARS, MODEL_MAX_CHARS, TAG_MAX_CHARS, TAG_MAX_COUNT, TITLE_MAX_CHARS,
        },
        vehicle::{Vehicle, VehicleFilter, VehicleModel as VehicleService},
        ModelError as WorkflowError,
    },
    settings::{BusinessSettings, MULTIPART_ENVELOPE_BYTES},
    views::{
        knowledge::{
            KnowledgeDetailPage, KnowledgeFilterValues, KnowledgeFormPage, KnowledgeFormValues,
            KnowledgeListPage,
        },
        vehicle::{AttachmentFormPage, AttachmentFormValues},
    },
};

mod attachments;
mod crud;
mod forms;
mod lifecycle;

use attachments::*;
use crud::*;
use forms::*;
use lifecycle::*;

pub fn routes() -> Routes {
    Routes::new()
        .add("/knowledge", get(list))
        .add(
            "/knowledge",
            post(create).layer(DefaultBodyLimit::max(FORM_BODY_LIMIT)),
        )
        .add("/knowledge/new", get(new_form))
        .add("/knowledge/{id}", get(show))
        .add("/knowledge/{id}/edit", get(edit_form))
        .add(
            "/knowledge/{id}/edit",
            post(update).layer(DefaultBodyLimit::max(FORM_BODY_LIMIT)),
        )
        .add("/knowledge/{id}/archive", post(archive).layer(body_limit()))
        .add("/knowledge/{id}/restore", post(restore).layer(body_limit()))
        .add("/knowledge/{id}/attachments/new", get(new_attachment_form))
        .add(
            "/knowledge/{id}/attachments",
            post(create_attachment).layer(DefaultBodyLimit::max(MULTIPART_ENVELOPE_BYTES)),
        )
        .add(
            "/knowledge/{id}/attachments/{attachment_id}/edit",
            get(edit_attachment_form),
        )
        .add(
            "/knowledge/{id}/attachments/{attachment_id}/edit",
            post(update_attachment).layer(body_limit()),
        )
        .add(
            "/knowledge/{id}/attachments/{attachment_id}/delete",
            post(delete_attachment).layer(body_limit()),
        )
}
