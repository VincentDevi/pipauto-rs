//! Consistent collection response envelope.

use serde::{Deserialize, Serialize};

use crate::domain::Page;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PaginationEnvelope<T> {
    pub items: Vec<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

impl<T, U> From<Page<U>> for PaginationEnvelope<T>
where
    T: From<U>,
{
    fn from(page: Page<U>) -> Self {
        Self {
            items: page.items.into_iter().map(T::from).collect(),
            next_cursor: page.next_cursor.map(|cursor| cursor.as_str().to_owned()),
        }
    }
}
