//! Server-rendered intervention discovery, draft, detail, and transition workflows.

use axum::{
    extract::{DefaultBodyLimit, Query},
    http::StatusCode,
    response::Response,
};
use chrono::{NaiveDate, NaiveTime, Utc};
use loco_rs::{
    controller::{
        extractor::shared_store::SharedStore, views::engines::TeraView, views::ViewEngine, Routes,
    },
    prelude::{get, post, Path},
    Result,
};

use crate::{
    auth::csrf::AuthenticatedAttachmentMultipart,
    controllers::browser::{
        context::{BrowserRequestContext, ResponsePreference},
        forms::{
            body_limit, invalid_format_error as validation_error, optional_text,
            parse_minor_units as parse_money_input, AuthenticatedForm, FormState,
        },
        responses,
    },
    domain::{
        AttachmentId, InterventionId, InterventionLineId, OpaqueCursor, Page, PageLimit,
        PageRequest, Quantity, ValidationError, ValidationErrors, VehicleId, WorkshopTime,
    },
    models::{
        attachment::{
            AttachmentMetadata, AttachmentModel as AttachmentService, AttachmentOwner,
            UploadAttachment, WriteAttachmentMetadata, CAPTION_MAX_CHARS, DISPLAY_NAME_MAX_CHARS,
        },
        customer::{ArchiveFilter, CustomerModel as CustomerService},
        intervention::{
            CreateIntervention, Intervention, InterventionFilter,
            InterventionModel as InterventionService, InterventionStatus, LineMoveDirection,
            LineMutationResult, UpdateIntervention, WriteLine,
        },
        intervention_line::{
            InterventionLine, InterventionLineCategory, DESCRIPTION_MAX_CHARS, UNIT_LABEL_MAX_CHARS,
        },
        vehicle::{Vehicle, VehicleFilter, VehicleModel as VehicleService},
        ModelError as WorkflowError,
    },
    settings::{BusinessSettings, MULTIPART_ENVELOPE_BYTES},
    views::{
        intervention::{
            InterventionDetailPage, InterventionFilterValues, InterventionFormPage,
            InterventionFormValues, InterventionLineFormPage, InterventionLineFormValues,
            InterventionLineRegion, InterventionListPage, InterventionTransitionPage,
        },
        vehicle::{AttachmentFormPage, AttachmentFormValues},
    },
};

mod attachments;
mod crud;
mod forms;
mod lines;
mod transitions;

use attachments::*;
use crud::*;
use lines::*;
use transitions::*;

pub fn routes() -> Routes {
    Routes::new()
        .add("/interventions", get(list))
        .add("/vehicles/{id}/interventions/new", get(new_form))
        .add(
            "/vehicles/{id}/interventions",
            post(create).layer(body_limit()),
        )
        .add("/interventions/{id}", get(show))
        .add("/interventions/{id}/edit", get(edit_form))
        .add("/interventions/{id}/edit", post(update).layer(body_limit()))
        .add("/interventions/{id}/lines/new", get(new_line_form))
        .add(
            "/interventions/{id}/lines",
            post(create_line).layer(body_limit()),
        )
        .add(
            "/interventions/{id}/lines/{line_id}/edit",
            get(edit_line_form),
        )
        .add(
            "/interventions/{id}/lines/{line_id}/edit",
            post(update_line).layer(body_limit()),
        )
        .add(
            "/interventions/{id}/lines/{line_id}/delete",
            post(delete_line).layer(body_limit()),
        )
        .add(
            "/interventions/{id}/lines/{line_id}/move-up",
            post(move_line_up).layer(body_limit()),
        )
        .add(
            "/interventions/{id}/lines/{line_id}/move-down",
            post(move_line_down).layer(body_limit()),
        )
        .add(
            "/interventions/{id}/attachments/new",
            get(new_attachment_form),
        )
        .add(
            "/interventions/{id}/attachments",
            post(create_attachment).layer(DefaultBodyLimit::max(MULTIPART_ENVELOPE_BYTES)),
        )
        .add(
            "/interventions/{id}/attachments/{attachment_id}/edit",
            get(edit_attachment_form),
        )
        .add(
            "/interventions/{id}/attachments/{attachment_id}/edit",
            post(update_attachment).layer(body_limit()),
        )
        .add(
            "/interventions/{id}/attachments/{attachment_id}/delete",
            post(delete_attachment).layer(body_limit()),
        )
        .add("/interventions/{id}/complete", get(complete_confirmation))
        .add(
            "/interventions/{id}/complete",
            post(complete).layer(body_limit()),
        )
        .add("/interventions/{id}/cancel", get(cancel_confirmation))
        .add(
            "/interventions/{id}/cancel",
            post(cancel).layer(body_limit()),
        )
}
