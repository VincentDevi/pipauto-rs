//! Intervention, line-item, transition, and deterministic service-history workflows.

use std::sync::Arc;

use chrono::{NaiveDate, Utc};

use crate::{
    domain::{
        CurrencyCode, CursorCodec, CursorResource, InterventionId, InterventionLineId, Money, Page,
        PageRequest, Quantity, ValidationCode, ValidationError, ValidationErrors, VehicleId,
    },
    models::{
        intervention::{
            validate_service_history_mileage, Intervention, InterventionModelError,
            InterventionStatus, NewIntervention, ServiceHistoryEntry, ServiceHistorySummary,
        },
        intervention_line::{
            InterventionLine, InterventionLineCategory, InterventionLineError, NewInterventionLine,
        },
        vehicle::Vehicle,
    },
    repositories::{
        intervention::{
            InterventionFilter, InterventionRepository, LineMutation, LineMutationResult,
        },
        vehicle::VehicleRepository,
    },
};

use super::WorkflowError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateIntervention {
    pub vehicle_id: VehicleId,
    pub service_date: NaiveDate,
    pub mileage: Option<u64>,
    pub customer_reported_problem: Option<String>,
    pub diagnostics: Option<String>,
    pub performed_work: Option<String>,
    pub recommendations: Option<String>,
    pub notes: Option<String>,
    pub currency: CurrencyCode,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UpdateIntervention {
    pub service_date: Option<NaiveDate>,
    pub mileage: Option<Option<u64>>,
    pub customer_reported_problem: Option<Option<String>>,
    pub diagnostics: Option<Option<String>>,
    pub performed_work: Option<Option<String>>,
    pub recommendations: Option<Option<String>>,
    pub notes: Option<Option<String>>,
    pub currency: Option<CurrencyCode>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteLine {
    pub category: InterventionLineCategory,
    pub description: String,
    pub quantity: Quantity,
    pub unit_label: String,
    pub unit_price_minor: i64,
    pub unit_cost_minor: Option<i64>,
    pub position: u32,
}

#[derive(Clone)]
pub struct InterventionService {
    interventions: Arc<dyn InterventionRepository>,
    vehicles: Arc<dyn VehicleRepository>,
    cursors: CursorCodec,
    resource: CursorResource,
}

impl InterventionService {
    pub fn new(
        interventions: Arc<dyn InterventionRepository>,
        vehicles: Arc<dyn VehicleRepository>,
        cursors: CursorCodec,
    ) -> Self {
        Self {
            interventions,
            vehicles,
            cursors,
            resource: CursorResource::parse("interventions").expect("static resource is valid"),
        }
    }

    pub async fn create(&self, command: CreateIntervention) -> Result<Intervention, WorkflowError> {
        let vehicle = self.require_vehicle(&command.vehicle_id).await?;
        if vehicle.is_archived() {
            return Err(WorkflowError::Conflict);
        }
        let now = Utc::now();
        let value = validate_intervention(command, now)?;
        let candidate = ServiceHistoryEntry {
            id: InterventionId::parse("pending_intervention")
                .expect("static candidate identifier is valid"),
            service_date: value.service_date,
            created_at: now,
            status: InterventionStatus::Draft,
            mileage: value.mileage,
        };
        self.validate_mileage(&candidate, &value.vehicle_id).await?;
        self.interventions.create(&value).await.map_err(Into::into)
    }

    pub async fn get(&self, id: &InterventionId) -> Result<Intervention, WorkflowError> {
        self.interventions
            .find_by_id(id)
            .await?
            .ok_or(WorkflowError::NotFound)
    }

    pub async fn update(
        &self,
        id: &InterventionId,
        command: UpdateIntervention,
    ) -> Result<Intervention, WorkflowError> {
        let current = self.get(id).await?;
        if current.status != InterventionStatus::Draft {
            return Err(WorkflowError::Conflict);
        }
        let value = validate_intervention(
            CreateIntervention {
                vehicle_id: current.vehicle_id.clone(),
                service_date: command.service_date.unwrap_or(current.service_date),
                mileage: command.mileage.unwrap_or(current.mileage),
                customer_reported_problem: command
                    .customer_reported_problem
                    .unwrap_or(current.customer_reported_problem),
                diagnostics: command.diagnostics.unwrap_or(current.diagnostics),
                performed_work: command.performed_work.unwrap_or(current.performed_work),
                recommendations: command.recommendations.unwrap_or(current.recommendations),
                notes: command.notes.unwrap_or(current.notes),
                currency: command.currency.unwrap_or(current.currency),
            },
            current.created_at,
        )?;
        let candidate = ServiceHistoryEntry {
            id: current.id.clone(),
            service_date: value.service_date,
            created_at: current.created_at,
            status: current.status,
            mileage: value.mileage,
        };
        self.validate_mileage(&candidate, &current.vehicle_id)
            .await?;
        self.interventions
            .update_draft(id, &value)
            .await
            .map_err(Into::into)
    }

    pub async fn list(
        &self,
        request: PageRequest<InterventionFilter>,
    ) -> Result<Page<ServiceHistorySummary>, WorkflowError> {
        self.page(request, None).await
    }

    pub async fn service_history(
        &self,
        vehicle_id: &VehicleId,
        mut request: PageRequest<InterventionFilter>,
    ) -> Result<Page<ServiceHistorySummary>, WorkflowError> {
        self.require_vehicle(vehicle_id).await?;
        request.filter.vehicle_id = Some(vehicle_id.clone());
        self.page(request, Some(vehicle_id)).await
    }

    pub async fn complete(&self, id: &InterventionId) -> Result<Intervention, WorkflowError> {
        let current = self.get(id).await?;
        if current.status != InterventionStatus::Draft {
            return Err(WorkflowError::Conflict);
        }
        if current.performed_work.is_none() {
            return Err(validation(
                "performed_work",
                ValidationCode::Required,
                "Record the work performed before completion.",
            ));
        }
        self.interventions
            .transition_draft(id, InterventionStatus::Completed)
            .await
            .map_err(Into::into)
    }

    pub async fn cancel(&self, id: &InterventionId) -> Result<Intervention, WorkflowError> {
        let current = self.get(id).await?;
        if current.status != InterventionStatus::Draft {
            return Err(WorkflowError::Conflict);
        }
        self.interventions
            .transition_draft(id, InterventionStatus::Cancelled)
            .await
            .map_err(Into::into)
    }

    pub async fn create_line(
        &self,
        intervention_id: &InterventionId,
        command: WriteLine,
    ) -> Result<LineMutationResult, WorkflowError> {
        let intervention = self.require_draft(intervention_id).await?;
        let line = validate_line(intervention_id, command, intervention.currency)?;
        self.interventions
            .mutate_line(intervention_id, LineMutation::Create(line))
            .await
            .map_err(Into::into)
    }

    pub async fn update_line(
        &self,
        intervention_id: &InterventionId,
        line_id: InterventionLineId,
        command: WriteLine,
    ) -> Result<LineMutationResult, WorkflowError> {
        let intervention = self.require_draft(intervention_id).await?;
        let line = validate_line(intervention_id, command, intervention.currency)?;
        self.interventions
            .mutate_line(intervention_id, LineMutation::Update { id: line_id, line })
            .await
            .map_err(Into::into)
    }

    pub async fn delete_line(
        &self,
        intervention_id: &InterventionId,
        line_id: InterventionLineId,
    ) -> Result<LineMutationResult, WorkflowError> {
        self.require_draft(intervention_id).await?;
        self.interventions
            .mutate_line(intervention_id, LineMutation::Delete { id: line_id })
            .await
            .map_err(Into::into)
    }

    pub async fn list_lines(
        &self,
        intervention_id: &InterventionId,
    ) -> Result<Vec<InterventionLine>, WorkflowError> {
        self.interventions
            .list_lines(intervention_id)
            .await
            .map_err(Into::into)
    }

    async fn page(
        &self,
        request: PageRequest<InterventionFilter>,
        vehicle_id: Option<&VehicleId>,
    ) -> Result<Page<ServiceHistorySummary>, WorkflowError> {
        let filter = request.filter;
        let after = request
            .after
            .as_ref()
            .map(|cursor| self.cursors.decode(cursor, &self.resource, &filter))
            .transpose()
            .map_err(|_| invalid_cursor())?;
        let page = if let Some(vehicle_id) = vehicle_id {
            self.interventions
                .vehicle_history(vehicle_id, &filter, request.limit, after.as_ref())
                .await?
        } else {
            self.interventions
                .list(&filter, request.limit, after.as_ref())
                .await?
        };
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

    async fn validate_mileage(
        &self,
        candidate: &ServiceHistoryEntry,
        vehicle_id: &VehicleId,
    ) -> Result<(), WorkflowError> {
        let neighbors = self
            .interventions
            .mileage_neighbors(candidate, vehicle_id)
            .await?;
        validate_service_history_mileage(candidate, &neighbors).map_err(|error| match error {
            InterventionModelError::MileageRegression => validation(
                "mileage",
                ValidationCode::InvalidFormat,
                "Mileage must fit between neighboring service-history records.",
            ),
            _ => WorkflowError::Internal,
        })
    }

    async fn require_vehicle(&self, id: &VehicleId) -> Result<Vehicle, WorkflowError> {
        self.vehicles
            .find_by_id(id)
            .await?
            .ok_or(WorkflowError::NotFound)
    }

    async fn require_draft(&self, id: &InterventionId) -> Result<Intervention, WorkflowError> {
        let intervention = self.get(id).await?;
        if intervention.status == InterventionStatus::Draft {
            Ok(intervention)
        } else {
            Err(WorkflowError::Conflict)
        }
    }
}

fn validate_intervention(
    command: CreateIntervention,
    now: chrono::DateTime<Utc>,
) -> Result<NewIntervention, WorkflowError> {
    if command.mileage.is_some_and(|value| value > i64::MAX as u64) {
        return Err(validation(
            "mileage",
            ValidationCode::InvalidFormat,
            "Enter a supported mileage.",
        ));
    }
    NewIntervention::new(
        command.vehicle_id,
        command.service_date,
        command.mileage,
        command.customer_reported_problem,
        command.diagnostics,
        command.performed_work,
        command.recommendations,
        command.notes,
        command.currency,
        now,
    )
    .map_err(intervention_validation)
}

fn validate_line(
    intervention_id: &InterventionId,
    command: WriteLine,
    currency: CurrencyCode,
) -> Result<NewInterventionLine, WorkflowError> {
    let unit_price = Money::new(command.unit_price_minor, currency)
        .map_err(|_| invalid_money("unit_price_minor"))?;
    let unit_cost = command
        .unit_cost_minor
        .map(|amount| Money::new(amount, currency))
        .transpose()
        .map_err(|_| invalid_money("unit_cost_minor"))?;
    NewInterventionLine::new(
        intervention_id.clone(),
        command.category,
        command.description,
        command.quantity,
        command.unit_label,
        unit_price,
        unit_cost,
        command.position,
        currency,
    )
    .map_err(line_validation)
}

fn intervention_validation(error: InterventionModelError) -> WorkflowError {
    match error {
        InterventionModelError::NarrativeTooLong => validation(
            "intervention",
            ValidationCode::TooLong,
            "Shorten the submitted workshop narrative.",
        ),
        _ => WorkflowError::Internal,
    }
}

fn line_validation(error: InterventionLineError) -> WorkflowError {
    let (field, code, message) = match error {
        InterventionLineError::Required => (
            "line",
            ValidationCode::Required,
            "Enter the required line details.",
        ),
        InterventionLineError::TooLong => (
            "line",
            ValidationCode::TooLong,
            "Shorten the submitted line value.",
        ),
        InterventionLineError::CurrencyMismatch => (
            "currency",
            ValidationCode::InvalidFormat,
            "Line currency must match the intervention.",
        ),
        InterventionLineError::Money(_) => {
            return invalid_money("line");
        }
    };
    validation(field, code, message)
}

fn invalid_money(field: &str) -> WorkflowError {
    validation(
        field,
        ValidationCode::InvalidFormat,
        "Enter a supported non-negative amount.",
    )
}

fn invalid_cursor() -> WorkflowError {
    validation(
        "cursor",
        ValidationCode::InvalidFormat,
        "Use the cursor returned by this history query.",
    )
}

fn validation(field: &str, code: ValidationCode, message: &str) -> WorkflowError {
    WorkflowError::Validation(ValidationErrors::one(
        ValidationError::new(field, code, message).expect("static validation metadata is valid"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intervention_service_rejects_negative_line_money_before_persistence() {
        let result = Money::new(-1, CurrencyCode::parse("EUR").expect("valid currency"));
        assert!(result.is_err());
        assert!(matches!(
            invalid_money("unit_price_minor"),
            WorkflowError::Validation(_)
        ));
    }
}
