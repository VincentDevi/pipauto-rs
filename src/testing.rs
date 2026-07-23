//! Test-only compatibility access to private persistence seams.
//!
//! Application and delivery code must use public model APIs. These exports exist only so the
//! existing backend-integrity suites can exercise concurrency and failure recovery directly.

pub mod persistence {
    pub use crate::models::persistence_error::PersistenceError as RepositoryError;

    pub mod attachment {
        pub use crate::models::attachment::repository::*;
    }

    pub mod auth {
        pub use crate::models::auth::repository::*;
    }

    pub mod calendar {
        pub use crate::models::intervention::calendar_repository::*;
    }

    pub mod customer {
        pub use crate::models::customer::repository::*;
    }

    pub mod intervention {
        pub use crate::models::intervention::repository::*;
    }

    pub mod invoice {
        pub use crate::models::invoice::repository::*;
    }

    pub mod technical_note {
        pub use crate::models::technical_note::repository::*;
    }

    pub mod vehicle {
        pub use crate::models::vehicle::repository::*;
    }

    pub mod surreal {
        pub mod attachment {
            pub use crate::models::attachment::persistence::*;
        }

        pub mod auth {
            pub use crate::models::auth::persistence::*;
        }

        pub mod customer {
            pub use crate::models::customer::persistence::*;
        }

        pub mod intervention {
            pub use crate::models::intervention::persistence::*;
        }

        pub mod invoice {
            pub use crate::models::invoice::persistence::*;
        }

        pub mod technical_note {
            pub use crate::models::technical_note::persistence::*;
        }

        pub mod vehicle {
            pub use crate::models::vehicle::persistence::*;
        }
    }
}
