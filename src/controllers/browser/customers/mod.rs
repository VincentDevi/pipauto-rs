//! Server-rendered customer workflows backed directly by application services.

use axum::{extract::Query, http::StatusCode, response::Response};
use loco_rs::{
    controller::{
        extractor::shared_store::SharedStore, views::engines::TeraView, views::ViewEngine, Routes,
    },
    prelude::{get, post, Path},
    Result,
};
use serde::Deserialize;

use crate::{
    controllers::browser::{
        context::BrowserRequestContext,
        forms::{body_limit, AuthenticatedForm, FormState},
        responses,
    },
    domain::{
        CustomerId, OpaqueCursor, Page, PageRequest, ValidationCode, ValidationError,
        ValidationErrors,
    },
    models::{
        customer::{
            ArchiveFilter, CreateCustomer, CustomerAddressInput, CustomerFilter,
            CustomerModel as CustomerService, UpdateCustomer, ADDRESS_LINE_MAX_CHARS,
            CITY_MAX_CHARS, DISPLAY_NAME_MAX_CHARS, EMAIL_MAX_CHARS, NOTES_MAX_CHARS,
            PHONE_MAX_CHARS, POSTAL_CODE_MAX_CHARS,
        },
        vehicle::{VehicleFilter, VehicleModel as VehicleService},
        ModelError as WorkflowError,
    },
    settings::BusinessSettings,
    views::customer::{CustomerDetailPage, CustomerFormPage, CustomerFormValues, CustomerListPage},
};

mod crud;
mod forms;
mod lifecycle;

use crud::*;
use lifecycle::*;

pub fn routes() -> Routes {
    Routes::new()
        .add("/customers", get(list))
        .add("/customers", post(create).layer(body_limit()))
        .add("/customers/new", get(new_form))
        .add("/customers/{id}", get(show))
        .add("/customers/{id}/edit", get(edit_form))
        .add("/customers/{id}/edit", post(update).layer(body_limit()))
        .add("/customers/{id}/archive", post(archive).layer(body_limit()))
        .add("/customers/{id}/restore", post(restore).layer(body_limit()))
}
