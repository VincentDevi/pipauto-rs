//! Server-rendered vehicle, service-history, and vehicle attachment workflows.

use axum::{
    extract::{DefaultBodyLimit, Query},
    http::StatusCode,
    response::Response,
};
use chrono::{Datelike, NaiveDate, Utc};
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
        AttachmentId, CustomerId, NormalizedRegistration, NormalizedVin, OpaqueCursor, Page,
        PageLimit, PageRequest, ValidationCode, ValidationError, ValidationErrors, VehicleId,
        WorkshopTime,
    },
    models::{
        attachment::{
            AttachmentMetadata, AttachmentModel as AttachmentService, AttachmentOwner,
            UploadAttachment, WriteAttachmentMetadata, CAPTION_MAX_CHARS, DISPLAY_NAME_MAX_CHARS,
        },
        customer::{ArchiveFilter, CustomerFilter, CustomerModel as CustomerService},
        intervention::{
            InterventionFilter, InterventionModel as InterventionService, InterventionStatus,
        },
        vehicle::{
            CreateVehicle, UpdateVehicle, VehicleFilter, VehicleModel as VehicleService,
            EARLIEST_VEHICLE_YEAR, ENGINE_TYPE_MAX_CHARS, MAKE_MAX_CHARS, MODEL_MAX_CHARS,
            NOTES_MAX_CHARS, REGISTRATION_MAX_CHARS, VIN_DISPLAY_MAX_CHARS,
        },
        ModelError as WorkflowError,
    },
    settings::{BusinessSettings, MULTIPART_ENVELOPE_BYTES},
    views::vehicle::{
        AttachmentFormPage, AttachmentFormValues, HistoryFilterValues, ReassignPage,
        ServiceHistoryPage, VehicleDetailPage, VehicleFilterValues, VehicleFormPage,
        VehicleFormValues, VehicleListPage,
    },
};

mod attachments;
mod crud;
mod forms;
mod history;
mod reassignment;

use attachments::*;
use crud::*;
use history::*;
use reassignment::*;

pub fn routes() -> Routes {
    Routes::new()
        .add("/vehicles", get(list))
        .add("/vehicles", post(create).layer(body_limit()))
        .add("/vehicles/new", get(generic_new_form))
        .add("/customers/{id}/vehicles/new", get(customer_new_form))
        .add("/vehicles/{id}", get(show))
        .add("/vehicles/{id}/edit", get(edit_form))
        .add("/vehicles/{id}/edit", post(update).layer(body_limit()))
        .add("/vehicles/{id}/archive", post(archive).layer(body_limit()))
        .add("/vehicles/{id}/restore", post(restore).layer(body_limit()))
        .add("/vehicles/{id}/reassign", get(reassign_form))
        .add(
            "/vehicles/{id}/reassign",
            post(reassign).layer(body_limit()),
        )
        .add("/vehicles/{id}/history", get(history))
        .add("/vehicles/{id}/attachments/new", get(new_attachment_form))
        .add(
            "/vehicles/{id}/attachments",
            post(create_attachment).layer(DefaultBodyLimit::max(MULTIPART_ENVELOPE_BYTES)),
        )
        .add("/attachments/{id}/edit", get(edit_attachment_form))
        .add(
            "/attachments/{id}/edit",
            post(update_attachment).layer(body_limit()),
        )
        .add(
            "/attachments/{id}/delete",
            post(delete_attachment).layer(body_limit()),
        )
}
