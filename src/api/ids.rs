//! String DTOs for table-specific identifiers.

use serde::{Deserialize, Serialize};

macro_rules! id_dto {
    ($dto:ident, $domain:ty) => {
        #[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
        #[serde(transparent)]
        pub struct $dto(pub String);

        impl From<&$domain> for $dto {
            fn from(value: &$domain) -> Self {
                Self(value.as_str().to_owned())
            }
        }
    };
}

id_dto!(CustomerIdDto, crate::domain::CustomerId);
id_dto!(VehicleIdDto, crate::domain::VehicleId);
id_dto!(InterventionIdDto, crate::domain::InterventionId);
id_dto!(TechnicalNoteIdDto, crate::domain::TechnicalNoteId);
id_dto!(AttachmentIdDto, crate::domain::AttachmentId);
id_dto!(InvoiceIdDto, crate::domain::InvoiceId);
id_dto!(PaymentIdDto, crate::domain::PaymentId);
