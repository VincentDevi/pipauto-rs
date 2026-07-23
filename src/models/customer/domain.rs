//! Customer model and write validation.

use chrono::{DateTime, Utc};

use crate::domain::{normalize_email, normalize_phone, normalize_search_text, CustomerId};

pub const DISPLAY_NAME_MAX_CHARS: usize = 160;
pub const EMAIL_MAX_CHARS: usize = 254;
pub const PHONE_MAX_CHARS: usize = 40;
pub const ADDRESS_LINE_MAX_CHARS: usize = 160;
pub const POSTAL_CODE_MAX_CHARS: usize = 32;
pub const CITY_MAX_CHARS: usize = 120;
pub const NOTES_MAX_CHARS: usize = 10_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Address {
    pub line_1: String,
    pub line_2: Option<String>,
    pub postal_code: String,
    pub city: String,
    pub country_code: String,
}

impl Address {
    /// Validate and trim a postal address.
    ///
    /// # Errors
    ///
    /// Rejects blank required values, oversized fields, and non-uppercase ISO alpha-2 country
    /// codes.
    pub fn new(
        line_1: String,
        line_2: Option<String>,
        postal_code: String,
        city: String,
        country_code: String,
    ) -> Result<Self, CustomerModelError> {
        let line_1 = required_text(line_1, ADDRESS_LINE_MAX_CHARS)?;
        let line_2 = optional_text(line_2, ADDRESS_LINE_MAX_CHARS)?;
        let postal_code = required_text(postal_code, POSTAL_CODE_MAX_CHARS)?;
        let city = required_text(city, CITY_MAX_CHARS)?;
        let country_code = country_code.trim().to_owned();
        if country_code.len() != 2 || !country_code.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(CustomerModelError::InvalidCountryCode);
        }
        Ok(Self {
            line_1,
            line_2,
            postal_code,
            city,
            country_code,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewCustomer {
    pub display_name: String,
    pub display_name_normalized: String,
    pub email: Option<String>,
    pub email_normalized: Option<String>,
    pub phone: Option<String>,
    pub phone_normalized: Option<String>,
    pub address: Option<Address>,
    pub notes: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Customer {
    pub id: CustomerId,
    pub display_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub address: Option<Address>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
}

impl Customer {
    #[must_use]
    pub const fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }
}

impl NewCustomer {
    /// Preserve display values while deriving deterministic lookup companions.
    ///
    /// # Errors
    ///
    /// Rejects invalid or oversized customer values.
    pub fn new(
        display_name: String,
        email: Option<String>,
        phone: Option<String>,
        address: Option<Address>,
        notes: Option<String>,
    ) -> Result<Self, CustomerModelError> {
        let display_name = required_text(display_name, DISPLAY_NAME_MAX_CHARS)?;
        let display_name_normalized = normalize_search_text(&display_name);
        let email = optional_text(email, EMAIL_MAX_CHARS)?;
        let email_normalized = email.as_deref().map(normalize_email);
        if email_normalized
            .as_deref()
            .is_some_and(|value| !valid_email(value))
        {
            return Err(CustomerModelError::InvalidEmail);
        }
        let phone = optional_text(phone, PHONE_MAX_CHARS)?;
        let phone_normalized = phone
            .as_deref()
            .map(normalize_phone)
            .transpose()
            .map_err(|_| CustomerModelError::InvalidPhone)?;
        let notes = optional_text(notes, NOTES_MAX_CHARS)?;
        Ok(Self {
            display_name,
            display_name_normalized,
            email,
            email_normalized,
            phone,
            phone_normalized,
            address,
            notes,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CustomerModelError {
    #[error("required customer text is blank")]
    Required,
    #[error("customer text exceeds its maximum length")]
    TooLong,
    #[error("customer email has an invalid format")]
    InvalidEmail,
    #[error("customer phone has an invalid format")]
    InvalidPhone,
    #[error("country code must be two uppercase ASCII letters")]
    InvalidCountryCode,
}

fn required_text(value: String, maximum: usize) -> Result<String, CustomerModelError> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Err(CustomerModelError::Required);
    }
    if value.chars().count() > maximum {
        return Err(CustomerModelError::TooLong);
    }
    Ok(value)
}

fn optional_text(
    value: Option<String>,
    maximum: usize,
) -> Result<Option<String>, CustomerModelError> {
    value
        .map(|value| {
            let value = value.trim().to_owned();
            if value.is_empty() {
                return Ok(None);
            }
            if value.chars().count() > maximum {
                return Err(CustomerModelError::TooLong);
            }
            Ok(Some(value))
        })
        .transpose()
        .map(Option::flatten)
}

fn valid_email(value: &str) -> bool {
    if value.contains(char::is_whitespace) {
        return false;
    }
    let mut parts = value.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    !local.is_empty() && domain.contains('.') && parts.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn customer_model_preserves_display_values_and_derives_lookups() {
        let customer = NewCustomer::new(
            "  Filippo  Straße ".into(),
            Some(" Filippo@Example.COM ".into()),
            Some(" +32 (0) 475-12.34.56 ".into()),
            None,
            Some("   ".into()),
        )
        .expect("valid customer");

        assert_eq!(customer.display_name, "Filippo  Straße");
        assert_eq!(customer.display_name_normalized, "filippo strasse");
        assert_eq!(customer.email.as_deref(), Some("Filippo@Example.COM"));
        assert_eq!(
            customer.email_normalized.as_deref(),
            Some("filippo@example.com")
        );
        assert_eq!(customer.phone.as_deref(), Some("+32 (0) 475-12.34.56"));
        assert_eq!(customer.phone_normalized.as_deref(), Some("+320475123456"));
        assert_eq!(customer.notes, None);

        let empty_optionals = NewCustomer::new(
            "Name".into(),
            Some("  ".into()),
            Some("  ".into()),
            None,
            None,
        )
        .expect("blank optional values are absent");
        assert_eq!(empty_optionals.email, None);
        assert_eq!(empty_optionals.email_normalized, None);
        assert_eq!(empty_optionals.phone, None);
        assert_eq!(empty_optionals.phone_normalized, None);
    }

    #[test]
    fn customer_model_validates_address_and_contact_bounds() {
        assert_eq!(
            Address::new(
                "Street".into(),
                None,
                "1000".into(),
                "Brussels".into(),
                "be".into()
            ),
            Err(CustomerModelError::InvalidCountryCode)
        );
        assert_eq!(
            NewCustomer::new(" ".into(), None, None, None, None),
            Err(CustomerModelError::Required)
        );
        assert_eq!(
            NewCustomer::new("Name".into(), Some("invalid".into()), None, None, None),
            Err(CustomerModelError::InvalidEmail)
        );
    }
}
