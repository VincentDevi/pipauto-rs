//! Central session and pre-authentication cookie construction.

use axum_extra::extract::cookie::{Cookie, SameSite};
use chrono::{DateTime, Utc};
use time::Duration as CookieDuration;

use super::settings::AuthSettings;

/// Environment-safe cookie factory.
#[derive(Clone)]
pub struct AuthCookies {
    settings: AuthSettings,
}

impl AuthCookies {
    /// Construct the cookie factory.
    #[must_use]
    pub fn new(settings: AuthSettings) -> Self {
        Self { settings }
    }

    /// Build the fixed-lifetime HttpOnly browser session cookie.
    #[must_use]
    pub fn session(&self, encoded_jwt: String) -> Cookie<'static> {
        Cookie::build((self.settings.session_cookie_name().to_owned(), encoded_jwt))
            .http_only(true)
            .secure(self.settings.secure_cookies())
            .same_site(SameSite::Lax)
            .path("/")
            .max_age(CookieDuration::seconds(
                i64::try_from(self.settings.session_lifetime().as_secs()).unwrap_or(i64::MAX),
            ))
            .build()
    }

    /// Build the short-lived HttpOnly login-CSRF nonce cookie.
    #[must_use]
    pub fn login_csrf(&self, nonce: String, _expires_at: DateTime<Utc>) -> Cookie<'static> {
        Cookie::build((self.settings.login_csrf_cookie_name().to_owned(), nonce))
            .http_only(true)
            .secure(self.settings.secure_cookies())
            .same_site(SameSite::Lax)
            .path("/")
            .max_age(CookieDuration::seconds(
                i64::try_from(self.settings.login_csrf_lifetime().as_secs()).unwrap_or(i64::MAX),
            ))
            .build()
    }

    /// Expire the selected session cookie using the same path and security attributes.
    #[must_use]
    pub fn clear_session(&self) -> Cookie<'static> {
        self.expired(self.settings.session_cookie_name())
    }

    /// Expire the pre-authentication nonce after a successful login.
    #[must_use]
    pub fn clear_login_csrf(&self) -> Cookie<'static> {
        self.expired(self.settings.login_csrf_cookie_name())
    }

    fn expired(&self, name: &str) -> Cookie<'static> {
        let mut cookie = Cookie::build((name.to_owned(), String::new()))
            .http_only(true)
            .secure(self.settings.secure_cookies())
            .same_site(SameSite::Lax)
            .path("/")
            .build();
        cookie.make_removal();
        cookie
    }
}

#[cfg(test)]
mod tests {
    use loco_rs::environment::Environment;

    use super::*;

    #[test]
    fn development_session_and_deletion_cookies_share_security_attributes() {
        let settings = AuthSettings::from_environment(&Environment::Test)
            .expect("test settings should be valid");
        let cookies = AuthCookies::new(settings);
        let session = cookies.session("secret-token".to_owned());
        assert_eq!(session.name(), "pipauto_session");
        assert_eq!(session.value(), "secret-token");
        assert_eq!(session.http_only(), Some(true));
        assert_eq!(session.secure(), Some(false));
        assert_eq!(session.same_site(), Some(SameSite::Lax));
        assert_eq!(session.path(), Some("/"));
        assert!(session.domain().is_none());

        let removal = cookies.clear_session();
        assert_eq!(removal.name(), session.name());
        assert_eq!(removal.http_only(), session.http_only());
        assert_eq!(removal.secure(), session.secure());
        assert_eq!(removal.same_site(), session.same_site());
        assert_eq!(removal.path(), session.path());
        assert_eq!(removal.max_age(), Some(CookieDuration::ZERO));
    }
}
