//! Persistence-neutral intervention and line-item repository contracts.

use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, Utc};

use crate::{
    domain::{
        CollectionFilter, CursorTuple, InterventionId, InterventionLineId, PageLimit, VehicleId,
    },
    models::{
        intervention::{
            Intervention, InterventionStatus, InterventionTotals, NewIntervention,
            ServiceHistoryEntry, ServiceHistorySummary,
        },
        intervention_line::{InterventionLine, NewInterventionLine},
    },
};

use super::{customer::RepositoryPage, RepositoryError};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InterventionFilter {
    pub vehicle_id: Option<VehicleId>,
    pub status: Option<InterventionStatus>,
    pub service_date_from: Option<DateTime<Utc>>,
    pub service_date_until: Option<DateTime<Utc>>,
}

impl CollectionFilter for InterventionFilter {
    fn fingerprint_bytes(&self) -> Vec<u8> {
        format!(
            "interventions:v2:{}:{:?}:{}:{}",
            self.vehicle_id.as_ref().map_or("", VehicleId::as_str),
            self.status,
            self.service_date_from.map_or(String::new(), |date| date
                .to_rfc3339_opts(SecondsFormat::Nanos, true)),
            self.service_date_until.map_or(String::new(), |date| date
                .to_rfc3339_opts(SecondsFormat::Nanos, true)),
        )
        .into_bytes()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LineMutation {
    Create(NewInterventionLine),
    Update {
        id: InterventionLineId,
        line: NewInterventionLine,
    },
    Delete {
        id: InterventionLineId,
    },
    Move {
        id: InterventionLineId,
        direction: LineMoveDirection,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineMoveDirection {
    Up,
    Down,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineMutationResult {
    pub line: Option<InterventionLine>,
    pub lines: Vec<InterventionLine>,
    pub totals: InterventionTotals,
}

#[async_trait]
pub trait InterventionRepository: Send + Sync {
    async fn create(&self, intervention: &NewIntervention)
        -> Result<Intervention, RepositoryError>;
    async fn find_by_id(
        &self,
        id: &InterventionId,
    ) -> Result<Option<Intervention>, RepositoryError>;
    async fn update_draft(
        &self,
        id: &InterventionId,
        intervention: &NewIntervention,
    ) -> Result<Intervention, RepositoryError>;
    async fn list(
        &self,
        filter: &InterventionFilter,
        limit: PageLimit,
        after: Option<&CursorTuple>,
    ) -> Result<RepositoryPage<ServiceHistorySummary>, RepositoryError>;
    async fn vehicle_history(
        &self,
        vehicle_id: &VehicleId,
        filter: &InterventionFilter,
        limit: PageLimit,
        after: Option<&CursorTuple>,
    ) -> Result<RepositoryPage<ServiceHistorySummary>, RepositoryError>;
    async fn mileage_neighbors(
        &self,
        candidate: &ServiceHistoryEntry,
        vehicle_id: &VehicleId,
    ) -> Result<Vec<ServiceHistoryEntry>, RepositoryError>;
    async fn transition_draft(
        &self,
        id: &InterventionId,
        target: InterventionStatus,
    ) -> Result<Intervention, RepositoryError>;
    async fn mutate_line(
        &self,
        intervention_id: &InterventionId,
        mutation: LineMutation,
    ) -> Result<LineMutationResult, RepositoryError>;
    async fn list_lines(
        &self,
        intervention_id: &InterventionId,
    ) -> Result<Vec<InterventionLine>, RepositoryError>;
    async fn line_workspace(
        &self,
        intervention_id: &InterventionId,
    ) -> Result<LineMutationResult, RepositoryError>;
}
