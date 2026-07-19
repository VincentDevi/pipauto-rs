//! Checked non-negative money values.

use std::fmt;

use thiserror::Error;

use super::Quantity;

/// Three-letter uppercase ISO 4217 currency code.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CurrencyCode([u8; 3]);

impl CurrencyCode {
    /// Parse an assigned uppercase ISO 4217 alphabetic code.
    ///
    /// # Errors
    ///
    /// Rejects lowercase, malformed, and unassigned codes.
    pub fn parse(value: &str) -> Result<Self, MoneyError> {
        const ISO_CODES: &str = " AED AFN ALL AMD ANG AOA ARS AUD AWG AZN BAM BBD BDT BGN BHD BIF BMD BND BOB BOV BRL BSD BTN BWP BYN BZD CAD CDF CHE CHF CHW CLF CLP CNY COP COU CRC CUC CUP CVE CZK DJF DKK DOP DZD EGP ERN ETB EUR FJD FKP GBP GEL GHS GIP GMD GNF GTQ GYD HKD HNL HRK HTG HUF IDR ILS INR IQD IRR ISK JMD JOD JPY KES KGS KHR KMF KPW KRW KWD KYD KZT LAK LBP LKR LRD LSL LYD MAD MDL MGA MKD MMK MNT MOP MRU MUR MVR MWK MXN MXV MYR MZN NAD NGN NIO NOK NPR NZD OMR PAB PEN PGK PHP PKR PLN PYG QAR RON RSD RUB RWF SAR SBD SCR SDG SEK SGD SHP SLE SLL SOS SRD SSP STN SVC SYP SZL THB TJS TMT TND TOP TRY TTD TWD TZS UAH UGX USD USN UYI UYU UYW UZS VED VES VND VUV WST XAF XAG XAU XBA XBB XBC XBD XCD XDR XOF XPD XPF XPT XSU XTS XUA XXX YER ZAR ZMW ZWG ";
        if value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(MoneyError::InvalidCurrency);
        }
        let needle = format!(" {value} ");
        if !ISO_CODES.contains(&needle) {
            return Err(MoneyError::InvalidCurrency);
        }
        let bytes = value.as_bytes();
        Ok(Self([bytes[0], bytes[1], bytes[2]]))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("currency codes contain validated ASCII")
    }
}

impl fmt::Debug for CurrencyCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("CurrencyCode")
            .field(&self.as_str())
            .finish()
    }
}

/// Non-negative monetary amount stored in the currency's minor units.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Money {
    minor_units: i64,
    currency: CurrencyCode,
}

impl Money {
    /// Construct a non-negative amount.
    pub fn new(minor_units: i64, currency: CurrencyCode) -> Result<Self, MoneyError> {
        if minor_units < 0 {
            return Err(MoneyError::Negative);
        }
        Ok(Self {
            minor_units,
            currency,
        })
    }

    #[must_use]
    pub const fn minor_units(self) -> i64 {
        self.minor_units
    }

    #[must_use]
    pub const fn currency(self) -> CurrencyCode {
        self.currency
    }

    /// Checked addition requiring matching currencies.
    pub fn checked_add(self, other: Self) -> Result<Self, MoneyError> {
        self.ensure_same_currency(other)?;
        let minor_units = self
            .minor_units
            .checked_add(other.minor_units)
            .ok_or(MoneyError::Overflow)?;
        Self::new(minor_units, self.currency)
    }

    /// Checked subtraction that cannot produce negative money.
    pub fn checked_sub(self, other: Self) -> Result<Self, MoneyError> {
        self.ensure_same_currency(other)?;
        let minor_units = self
            .minor_units
            .checked_sub(other.minor_units)
            .ok_or(MoneyError::Overflow)?;
        Self::new(minor_units, self.currency)
    }

    /// Multiply by a quantity, rounding half-up to the nearest minor unit.
    pub fn checked_mul_quantity(self, quantity: Quantity) -> Result<Self, MoneyError> {
        let product = i128::from(self.minor_units)
            .checked_mul(i128::from(quantity.thousandths()))
            .ok_or(MoneyError::Overflow)?;
        let rounded = product.checked_add(500).ok_or(MoneyError::Overflow)? / 1_000;
        let minor_units = i64::try_from(rounded).map_err(|_| MoneyError::Overflow)?;
        Self::new(minor_units, self.currency)
    }

    fn ensure_same_currency(self, other: Self) -> Result<(), MoneyError> {
        if self.currency == other.currency {
            Ok(())
        } else {
            Err(MoneyError::CurrencyMismatch)
        }
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum MoneyError {
    #[error("currency must be an assigned uppercase ISO 4217 code")]
    InvalidCurrency,
    #[error("money cannot be negative")]
    Negative,
    #[error("money arithmetic overflowed")]
    Overflow,
    #[error("money currencies do not match")]
    CurrencyMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eur() -> CurrencyCode {
        CurrencyCode::parse("EUR").expect("EUR is valid")
    }

    #[test]
    fn domain_money_enforces_currency_sign_and_checked_arithmetic() {
        assert_eq!(CurrencyCode::parse("eur"), Err(MoneyError::InvalidCurrency));
        assert_eq!(CurrencyCode::parse("ZZZ"), Err(MoneyError::InvalidCurrency));
        assert_eq!(Money::new(-1, eur()), Err(MoneyError::Negative));
        let maximum = Money::new(i64::MAX, eur()).expect("maximum is non-negative");
        let one = Money::new(1, eur()).expect("one is valid");
        assert_eq!(maximum.checked_add(one), Err(MoneyError::Overflow));
        assert_eq!(one.checked_sub(maximum), Err(MoneyError::Negative));
    }

    #[test]
    fn domain_money_quantity_multiplication_rounds_half_up_at_boundaries() {
        let one_cent = Money::new(1, eur()).expect("valid money");
        let below_half = Quantity::parse("1.499").expect("valid quantity");
        let at_half = Quantity::parse("1.500").expect("valid quantity");
        assert_eq!(
            one_cent
                .checked_mul_quantity(below_half)
                .expect("fits")
                .minor_units(),
            1
        );
        assert_eq!(
            one_cent
                .checked_mul_quantity(at_half)
                .expect("fits")
                .minor_units(),
            2
        );

        let maximum = Money::new(i64::MAX, eur()).expect("valid money");
        assert_eq!(
            maximum.checked_mul_quantity(Quantity::parse("2").expect("valid quantity")),
            Err(MoneyError::Overflow)
        );
    }
}
