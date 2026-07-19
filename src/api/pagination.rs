//! Consistent collection response envelope.

use serde::{Deserialize, Serialize};

use crate::domain::Page;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PaginationEnvelope<T> {
    pub data: Vec<T>,
    pub next_cursor: Option<String>,
}

impl<T, U> From<Page<U>> for PaginationEnvelope<T>
where
    T: From<U>,
{
    fn from(page: Page<U>) -> Self {
        Self {
            data: page.items.into_iter().map(T::from).collect(),
            next_cursor: page.next_cursor.map(|cursor| cursor.as_str().to_owned()),
        }
    }
}
