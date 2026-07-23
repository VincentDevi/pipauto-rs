//! Server-rendered login and logout browser flow.

use crate::{
    auth::{
        cookies::AuthCookies,
        csrf::{CsrfService, GuestLoginCsrfForm, LogoutCsrfForm},
        extractors::{append_vary_hx_request, safe_next_destination, OptionalCurrentUser},
    },
    models::auth::{AuthenticationModel as AuthService, LoginCommand, LoginError},
    views::auth::{AuthenticationUnavailableView, LoginView},
};
use axum::{
    extract::{ConnectInfo, DefaultBodyLimit, Extension, Query},
    http::{
        header::{CACHE_CONTROL, LOCATION, RETRY_AFTER},
        HeaderMap, HeaderValue, StatusCode,
    },
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::CookieJar;
use chrono::Utc;
use loco_rs::{
    controller::extractor::shared_store::SharedStore,
    controller::{format, views::engines::TeraView, views::ViewEngine, Routes},
    prelude::{get, post},
    Result,
};
use serde::Deserialize;
use std::{fmt::Write as _, net::SocketAddr};

const AUTH_FORM_BODY_LIMIT: usize = 4 * 1_024;
const MAX_RETRY_AFTER_SECONDS: i64 = 300;

mod login;
mod logout;

use login::*;
use logout::*;

/// Guest-only sign-in routes.
#[must_use]
pub fn guest_routes() -> Routes {
    Routes::new().add("/login", get(show_login)).add(
        "/login",
        post(login).layer(DefaultBodyLimit::max(AUTH_FORM_BODY_LIMIT)),
    )
}

/// Authenticated sign-out route.
#[must_use]
pub fn authenticated_routes() -> Routes {
    Routes::new().add(
        "/logout",
        post(logout).layer(DefaultBodyLimit::max(AUTH_FORM_BODY_LIMIT)),
    )
}
