//! Database-independent vehicle model and write validation.

use crate::domain::{normalize_search_text, CustomerId, NormalizedRegistration, NormalizedVin};

pub const MAKE_MAX_CHARS: usize = 80;
pub const MODEL_MAX_CHARS: usize = 80;
pub const REGISTRATION_MAX_CHARS: usize = 40;
pub const VIN_DISPLAY_MAX_CHARS: usize = 40;
pub const ENGINE_TYPE_MAX_CHARS: usize = 160;
pub const NOTES_MAX_CHARS: usize = 10_000;
pub const EARLIEST_VEHICLE_YEAR: i32 = 1886;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewVehicle {
    pub customer_id: CustomerId,
    pub make: String,
    pub make_normalized: String,
    pub model: String,
    pub model_normalized: String,
    pub year: Option<i32>,
    pub registration: Option<String>,
    pub registration_normalized: Option<NormalizedRegistration>,
    pub vin: Option<String>,
    pub vin_normalized: Option<NormalizedVin>,
    pub current_mileage: Option<u64>,
    pub engine_type: Option<String>,
    pub notes: Option<String>,
}

impl NewVehicle {
    /// Preserve vehicle display values while deriving deterministic lookup companions.
    ///
    /// `current_year` is supplied by the caller so year validation is deterministic in tests.
    ///
    /// # Errors
    ///
    /// Rejects invalid, blank, oversized, or implausible vehicle values.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        customer_id: CustomerId,
        make: String,
        model: String,
        year: Option<i32>,
        registration: Option<String>,
        vin: Option<String>,
        current_mileage: Option<u64>,
        engine_type: Option<String>,
        notes: Option<String>,
        current_year: i32,
    ) -> Result<Self, VehicleModelError> {
        let make = required_text(make, MAKE_MAX_CHARS)?;
        let model = required_text(model, MODEL_MAX_CHARS)?;
        if year.is_some_and(|year| !(EARLIEST_VEHICLE_YEAR..=current_year + 1).contains(&year)) {
            return Err(VehicleModelError::InvalidYear);
        }
        let registration = optional_text(registration, REGISTRATION_MAX_CHARS)?;
        let registration_normalized = registration
            .as_deref()
            .map(NormalizedRegistration::parse)
            .transpose()
            .map_err(|_| VehicleModelError::InvalidRegistration)?;
        let vin = optional_text(vin, VIN_DISPLAY_MAX_CHARS)?;
        let vin_normalized = vin
            .as_deref()
            .map(NormalizedVin::parse)
            .transpose()
            .map_err(|_| VehicleModelError::InvalidVin)?;
        let engine_type = optional_text(engine_type, ENGINE_TYPE_MAX_CHARS)?;
        let notes = optional_text(notes, NOTES_MAX_CHARS)?;
        Ok(Self {
            customer_id,
            make_normalized: normalize_search_text(&make),
            make,
            model_normalized: normalize_search_text(&model),
            model,
            year,
            registration,
            registration_normalized,
            vin,
            vin_normalized,
            current_mileage,
            engine_type,
            notes,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum VehicleModelError {
    #[error("required vehicle text is blank")]
    Required,
    #[error("vehicle text exceeds its maximum length")]
    TooLong,
    #[error("vehicle year is outside the plausible range")]
    InvalidYear,
    #[error("registration has an invalid format")]
    InvalidRegistration,
    #[error("VIN has an invalid format")]
    InvalidVin,
}

fn required_text(value: String, maximum: usize) -> Result<String, VehicleModelError> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Err(VehicleModelError::Required);
    }
    if value.chars().count() > maximum {
        return Err(VehicleModelError::TooLong);
    }
    Ok(value)
}

fn optional_text(
    value: Option<String>,
    maximum: usize,
) -> Result<Option<String>, VehicleModelError> {
    value
        .map(|value| {
            let value = value.trim().to_owned();
            if value.is_empty() {
                return Ok(None);
            }
            if value.chars().count() > maximum {
                return Err(VehicleModelError::TooLong);
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
    fn vehicle_model_preserves_display_values_and_derives_lookups() {
        let vehicle = NewVehicle::new(
            CustomerId::parse("filippo").expect("valid customer id"),
            " Volkswagen ".into(),
            " Golf  GTE ".into(),
            Some(2025),
            Some(" 1-abc-234 ".into()),
            Some(" wvwzzz1jzxw000001 ".into()),
            Some(125_000),
            Some(" 1.4 TSI hybrid ".into()),
            Some("  ".into()),
            2025,
        )
        .expect("valid vehicle");

        assert_eq!(vehicle.make, "Volkswagen");
        assert_eq!(vehicle.make_normalized, "volkswagen");
        assert_eq!(vehicle.model, "Golf  GTE");
        assert_eq!(vehicle.model_normalized, "golf gte");
        assert_eq!(vehicle.registration.as_deref(), Some("1-abc-234"));
        assert_eq!(
            vehicle
                .registration_normalized
                .as_ref()
                .map(NormalizedRegistration::as_str),
            Some("1ABC234")
        );
        assert_eq!(vehicle.vin.as_deref(), Some("wvwzzz1jzxw000001"));
        assert_eq!(
            vehicle.vin_normalized.as_ref().map(NormalizedVin::as_str),
            Some("WVWZZZ1JZXW000001")
        );
        assert_eq!(vehicle.notes, None);

        let empty_identifiers = NewVehicle::new(
            CustomerId::parse("filippo").expect("valid customer id"),
            "Volkswagen".into(),
            "Golf".into(),
            None,
            Some("  ".into()),
            Some("  ".into()),
            None,
            None,
            None,
            2025,
        )
        .expect("blank identifiers are absent");
        assert_eq!(empty_identifiers.registration, None);
        assert_eq!(empty_identifiers.registration_normalized, None);
        assert_eq!(empty_identifiers.vin, None);
        assert_eq!(empty_identifiers.vin_normalized, None);
    }

    #[test]
    fn vehicle_model_rejects_invalid_year_and_identifiers() {
        let customer = CustomerId::parse("filippo").expect("valid customer id");
        assert_eq!(
            NewVehicle::new(
                customer.clone(),
                "Make".into(),
                "Model".into(),
                Some(2027),
                None,
                None,
                None,
                None,
                None,
                2025,
            ),
            Err(VehicleModelError::InvalidYear)
        );
        assert_eq!(
            NewVehicle::new(
                customer,
                "Make".into(),
                "Model".into(),
                None,
                None,
                Some("WVWZZZ1JZXW00000I".into()),
                None,
                None,
                None,
                2025,
            ),
            Err(VehicleModelError::InvalidVin)
        );
    }
}
