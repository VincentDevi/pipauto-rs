//! Customer application workflows.

use std::sync::Arc;

use crate::{
    domain::{
        normalize_search_text, CursorCodec, CursorResource, CustomerId, Page, PageRequest,
        ValidationCode, ValidationError, ValidationErrors,
    },
    models::customer::{Address, Customer, CustomerModelError, NewCustomer},
    repositories::customer::{CustomerFilter, CustomerRepository},
};

use super::WorkflowError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateCustomer {
    pub display_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub address: Option<CustomerAddressInput>,
    pub notes: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomerAddressInput {
    pub line_1: String,
    pub line_2: Option<String>,
    pub postal_code: String,
    pub city: String,
    pub country_code: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UpdateCustomer {
    pub display_name: Option<String>,
    pub email: Option<Option<String>>,
    pub phone: Option<Option<String>>,
    pub address: Option<Option<CustomerAddressInput>>,
    pub notes: Option<Option<String>>,
}

#[derive(Clone)]
pub struct CustomerService {
    repository: Arc<dyn CustomerRepository>,
    cursors: CursorCodec,
    resource: CursorResource,
}

impl CustomerService {
    pub fn new(repository: Arc<dyn CustomerRepository>, cursors: CursorCodec) -> Self {
        Self {
            repository,
            cursors,
            resource: CursorResource::parse("customers").expect("static resource is valid"),
        }
    }

    pub async fn create(&self, command: CreateCustomer) -> Result<Customer, WorkflowError> {
        let address = validate_address(command.address)?;
        let customer = validate_customer(
            command.display_name,
            command.email,
            command.phone,
            address,
            command.notes,
        )?;
        self.repository.create(&customer).await.map_err(Into::into)
    }

    pub async fn get(&self, id: &CustomerId) -> Result<Customer, WorkflowError> {
        self.repository
            .find_by_id(id)
            .await?
            .ok_or(WorkflowError::NotFound)
    }

    pub async fn update(
        &self,
        id: &CustomerId,
        command: UpdateCustomer,
    ) -> Result<Customer, WorkflowError> {
        let current = self.get(id).await?;
        let address = match command.address {
            Some(address) => validate_address(address)?,
            None => current.address,
        };
        let customer = validate_customer(
            command.display_name.unwrap_or(current.display_name),
            command.email.unwrap_or(current.email),
            command.phone.unwrap_or(current.phone),
            address,
            command.notes.unwrap_or(current.notes),
        )?;
        self.repository
            .update(id, &customer)
            .await
            .map_err(Into::into)
    }

    pub async fn list(
        &self,
        request: PageRequest<CustomerFilter>,
    ) -> Result<Page<Customer>, WorkflowError> {
        let mut filter = request.filter;
        filter.query = filter
            .query
            .map(|query| normalize_search_text(&query))
            .filter(|query| !query.is_empty());
        let after = request
            .after
            .as_ref()
            .map(|cursor| self.cursors.decode(cursor, &self.resource, &filter))
            .transpose()
            .map_err(|_| validation("cursor", "Use the cursor returned by this search."))?;
        let page = self
            .repository
            .list(&filter, request.limit, after.as_ref())
            .await?;
        let next_cursor = page
            .next
            .as_ref()
            .map(|tuple| self.cursors.encode(&self.resource, tuple, &filter))
            .transpose()
            .map_err(|_| WorkflowError::Internal)?;
        Ok(Page {
            items: page.items,
            next_cursor,
        })
    }

    pub async fn archive(&self, id: &CustomerId) -> Result<Customer, WorkflowError> {
        self.repository.archive(id).await.map_err(Into::into)
    }

    pub async fn restore(&self, id: &CustomerId) -> Result<Customer, WorkflowError> {
        self.repository.restore(id).await.map_err(Into::into)
    }
}

fn validate_address(value: Option<CustomerAddressInput>) -> Result<Option<Address>, WorkflowError> {
    value
        .map(|address| {
            Address::new(
                address.line_1,
                address.line_2,
                address.postal_code,
                address.city,
                address.country_code,
            )
            .map_err(customer_validation)
        })
        .transpose()
}

fn validate_customer(
    display_name: String,
    email: Option<String>,
    phone: Option<String>,
    address: Option<Address>,
    notes: Option<String>,
) -> Result<NewCustomer, WorkflowError> {
    NewCustomer::new(display_name, email, phone, address, notes).map_err(customer_validation)
}

fn customer_validation(error: CustomerModelError) -> WorkflowError {
    let (field, message) = match error {
        CustomerModelError::Required => ("display_name", "Enter a customer name."),
        CustomerModelError::TooLong => ("customer", "Shorten the submitted value."),
        CustomerModelError::InvalidEmail => ("email", "Enter a valid email address."),
        CustomerModelError::InvalidPhone => ("phone", "Enter a valid phone number."),
        CustomerModelError::InvalidCountryCode => (
            "address.country_code",
            "Use a two-letter uppercase country code.",
        ),
    };
    validation(field, message)
}

fn validation(field: &str, message: &str) -> WorkflowError {
    WorkflowError::Validation(ValidationErrors::one(
        ValidationError::new(field, ValidationCode::InvalidFormat, message)
            .expect("static validation metadata is valid"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn customer_service_maps_model_errors_to_public_fields() {
        let error = customer_validation(CustomerModelError::InvalidEmail);
        let WorkflowError::Validation(errors) = error else {
            panic!("expected validation")
        };
        assert_eq!(errors.as_slice()[0].field().as_str(), "email");
    }
}
