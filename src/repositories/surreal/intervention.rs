//! SurrealDB intervention repository adapter.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use surrealdb::{
    engine::any::Any,
    types::{RecordId, SurrealValue},
    Surreal,
};

use crate::{
    domain::{
        CurrencyCode, CursorSortValue, CursorTuple, InterventionId, InterventionLineId, Money,
        PageLimit, Quantity, VehicleId,
    },
    models::{
        intervention::{
            Intervention, InterventionStatus, InterventionTotals, NewIntervention,
            ServiceHistoryEntry, ServiceHistorySummary,
        },
        intervention_line::{InterventionLine, InterventionLineCategory, NewInterventionLine},
    },
    repositories::{
        customer::RepositoryPage,
        intervention::{
            InterventionFilter, InterventionRepository, LineMutation, LineMutationResult,
        },
        RepositoryError,
    },
};

use super::support;

const INTERVENTION_PROJECTION: &str = "id, vehicle, service_date, status, mileage, customer_reported_problem, diagnostics, performed_work, recommendations, notes, currency, created_at, updated_at, completed_at, cancelled_at";
const LINE_PROJECTION: &str = "id, intervention, category, description, type::int(quantity * 1000dec) AS quantity_thousandths, unit_label, currency, unit_price_minor, unit_cost_minor, total_price_minor, total_cost_minor, position, created_at, updated_at";

#[derive(Clone)]
pub struct SurrealInterventionRepository {
    client: Surreal<Any>,
}

impl SurrealInterventionRepository {
    #[must_use]
    pub fn new(client: Surreal<Any>) -> Self {
        Self { client }
    }

    async fn totals(&self, id: &InterventionId) -> Result<InterventionTotals, RepositoryError> {
        totals_with_client(&self.client, id).await
    }

    async fn list_internal(
        &self,
        filter: &InterventionFilter,
        limit: PageLimit,
        after: Option<&CursorTuple>,
    ) -> Result<RepositoryPage<ServiceHistorySummary>, RepositoryError> {
        let (after_date, after_created_at, after_id) = after
            .map(history_cursor_values)
            .transpose()?
            .map_or((None, None, None), |(date, created_at, id)| {
                (Some(date), Some(created_at), Some(id))
            });
        let vehicle = filter
            .vehicle_id
            .as_ref()
            .map(|id| support::record_id("vehicle", id.as_str()))
            .transpose()?;
        let query = format!(
            "SELECT {INTERVENTION_PROJECTION} FROM intervention WHERE ($vehicle IS NONE OR vehicle = $vehicle) AND ($status IS NONE OR status = $status) AND ($date_from IS NONE OR service_date >= $date_from) AND ($date_to IS NONE OR service_date <= $date_to) AND ($after_date IS NONE OR service_date < $after_date OR (service_date = $after_date AND created_at < $after_created_at) OR (service_date = $after_date AND created_at = $after_created_at AND id < $after_id)) ORDER BY service_date DESC, created_at DESC, id DESC LIMIT $fetch_limit;"
        );
        let mut response = support::checked_response(
            self.client
                .query(query)
                .bind(("vehicle", vehicle))
                .bind(("status", filter.status.map(status_value).map(str::to_owned)))
                .bind(("date_from", filter.service_date_from.map(midnight)))
                .bind(("date_to", filter.service_date_to.map(midnight)))
                .bind(("after_date", after_date.map(midnight)))
                .bind(("after_created_at", after_created_at))
                .bind(("after_id", after_id))
                .bind(("fetch_limit", i64::from(limit.value()) + 1))
                .await,
        )?;
        let mut rows: Vec<DbIntervention> = support::take(&mut response, 0)?;
        let has_more = rows.len() > usize::from(limit.value());
        if has_more {
            rows.pop();
        }
        let next = if has_more {
            rows.last().map(history_cursor).transpose()?
        } else {
            None
        };
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let intervention: Intervention = row.try_into()?;
            let totals = self.totals(&intervention.id).await?;
            items.push(ServiceHistorySummary {
                intervention,
                totals,
            });
        }
        Ok(RepositoryPage { items, next })
    }
}

#[derive(Deserialize, SurrealValue)]
struct DbIntervention {
    id: RecordId,
    vehicle: RecordId,
    service_date: DateTime<Utc>,
    status: String,
    mileage: Option<i64>,
    customer_reported_problem: Option<String>,
    diagnostics: Option<String>,
    performed_work: Option<String>,
    recommendations: Option<String>,
    notes: Option<String>,
    currency: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    cancelled_at: Option<DateTime<Utc>>,
}

impl TryFrom<DbIntervention> for Intervention {
    type Error = RepositoryError;

    fn try_from(value: DbIntervention) -> Result<Self, Self::Error> {
        Ok(Self {
            id: InterventionId::parse(support::record_key(&value.id, "intervention")?)
                .map_err(|_| RepositoryError::CorruptData)?,
            vehicle_id: VehicleId::parse(support::record_key(&value.vehicle, "vehicle")?)
                .map_err(|_| RepositoryError::CorruptData)?,
            service_date: value.service_date.date_naive(),
            status: parse_status(&value.status)?,
            mileage: value
                .mileage
                .map(u64::try_from)
                .transpose()
                .map_err(|_| RepositoryError::CorruptData)?,
            customer_reported_problem: value.customer_reported_problem,
            diagnostics: value.diagnostics,
            performed_work: value.performed_work,
            recommendations: value.recommendations,
            notes: value.notes,
            currency: CurrencyCode::parse(&value.currency)
                .map_err(|_| RepositoryError::CorruptData)?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            completed_at: value.completed_at,
            cancelled_at: value.cancelled_at,
        })
    }
}

#[derive(Deserialize, SurrealValue)]
struct DbLine {
    id: RecordId,
    intervention: RecordId,
    category: String,
    description: String,
    quantity_thousandths: i64,
    unit_label: String,
    currency: String,
    unit_price_minor: i64,
    unit_cost_minor: Option<i64>,
    total_price_minor: i64,
    total_cost_minor: Option<i64>,
    position: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<DbLine> for InterventionLine {
    type Error = RepositoryError;

    fn try_from(value: DbLine) -> Result<Self, Self::Error> {
        let currency =
            CurrencyCode::parse(&value.currency).map_err(|_| RepositoryError::CorruptData)?;
        Ok(Self {
            id: InterventionLineId::parse(support::record_key(&value.id, "intervention_line")?)
                .map_err(|_| RepositoryError::CorruptData)?,
            intervention_id: InterventionId::parse(support::record_key(
                &value.intervention,
                "intervention",
            )?)
            .map_err(|_| RepositoryError::CorruptData)?,
            category: parse_category(&value.category)?,
            description: value.description,
            quantity: Quantity::from_thousandths(
                u64::try_from(value.quantity_thousandths)
                    .map_err(|_| RepositoryError::CorruptData)?,
            )
            .map_err(|_| RepositoryError::CorruptData)?,
            unit_label: value.unit_label,
            unit_price: Money::new(value.unit_price_minor, currency)
                .map_err(|_| RepositoryError::CorruptData)?,
            unit_cost: value
                .unit_cost_minor
                .map(|amount| Money::new(amount, currency))
                .transpose()
                .map_err(|_| RepositoryError::CorruptData)?,
            total_price: Money::new(value.total_price_minor, currency)
                .map_err(|_| RepositoryError::CorruptData)?,
            total_cost: value
                .total_cost_minor
                .map(|amount| Money::new(amount, currency))
                .transpose()
                .map_err(|_| RepositoryError::CorruptData)?,
            position: u32::try_from(value.position).map_err(|_| RepositoryError::CorruptData)?,
            created_at: value.created_at,
            updated_at: value.updated_at,
        })
    }
}

#[derive(Deserialize, SurrealValue)]
struct DbCurrency {
    currency: String,
}

#[async_trait]
impl InterventionRepository for SurrealInterventionRepository {
    async fn create(&self, value: &NewIntervention) -> Result<Intervention, RepositoryError> {
        let transaction = self
            .client
            .clone()
            .begin()
            .await
            .map_err(|error| support::classify_query_error(&error))?;
        let result = async {
            let mut active = support::checked_response(
                transaction
                    .query("SELECT VALUE id FROM ONLY $vehicle WHERE archived_at IS NONE;")
                    .bind(("vehicle", support::record_id("vehicle", value.vehicle_id.as_str())?))
                    .await,
            )?;
            let found: Option<RecordId> = support::take(&mut active, 0)?;
            if found.is_none() {
                return Err(RepositoryError::Conflict);
            }
            let mut response = write_intervention_query(
                &transaction,
                "CREATE intervention SET vehicle = $vehicle, service_date = $service_date, mileage = $mileage, customer_reported_problem = $customer_reported_problem, diagnostics = $diagnostics, performed_work = $performed_work, recommendations = $recommendations, notes = $notes, currency = $currency RETURN AFTER;",
                None,
                value,
            )
            .await?;
            let row: Option<DbIntervention> = support::take(&mut response, 0)?;
            let intervention: Intervention = row.ok_or(RepositoryError::CorruptData)?.try_into()?;
            raise_vehicle_mileage(&transaction, &value.vehicle_id, value.mileage).await?;
            Ok(intervention)
        }
        .await;
        finish_transaction(transaction, result).await
    }

    async fn find_by_id(
        &self,
        id: &InterventionId,
    ) -> Result<Option<Intervention>, RepositoryError> {
        let query = format!("SELECT {INTERVENTION_PROJECTION} FROM ONLY $record;");
        let mut response = support::checked_response(
            self.client
                .query(query)
                .bind(("record", support::record_id("intervention", id.as_str())?))
                .await,
        )?;
        let row: Option<DbIntervention> = support::take(&mut response, 0)?;
        row.map(TryInto::try_into).transpose()
    }

    async fn update_draft(
        &self,
        id: &InterventionId,
        value: &NewIntervention,
    ) -> Result<Intervention, RepositoryError> {
        let transaction = self
            .client
            .clone()
            .begin()
            .await
            .map_err(|error| support::classify_query_error(&error))?;
        let result = async {
            let mut response = write_intervention_query(
                &transaction,
                "UPDATE ONLY $record SET vehicle = $vehicle, service_date = $service_date, mileage = $mileage, customer_reported_problem = $customer_reported_problem, diagnostics = $diagnostics, performed_work = $performed_work, recommendations = $recommendations, notes = $notes, currency = $currency WHERE status = 'draft' RETURN AFTER;",
                Some(support::record_id("intervention", id.as_str())?),
                value,
            )
            .await?;
            let row: Option<DbIntervention> = support::take(&mut response, 0)?;
            let intervention: Intervention = row.ok_or(RepositoryError::Conflict)?.try_into()?;
            raise_vehicle_mileage(&transaction, &value.vehicle_id, value.mileage).await?;
            Ok(intervention)
        }
        .await;
        finish_transaction(transaction, result).await
    }

    async fn list(
        &self,
        filter: &InterventionFilter,
        limit: PageLimit,
        after: Option<&CursorTuple>,
    ) -> Result<RepositoryPage<ServiceHistorySummary>, RepositoryError> {
        self.list_internal(filter, limit, after).await
    }

    async fn vehicle_history(
        &self,
        vehicle_id: &VehicleId,
        filter: &InterventionFilter,
        limit: PageLimit,
        after: Option<&CursorTuple>,
    ) -> Result<RepositoryPage<ServiceHistorySummary>, RepositoryError> {
        let mut filter = filter.clone();
        filter.vehicle_id = Some(vehicle_id.clone());
        self.list_internal(&filter, limit, after).await
    }

    async fn mileage_neighbors(
        &self,
        candidate: &ServiceHistoryEntry,
        vehicle_id: &VehicleId,
    ) -> Result<Vec<ServiceHistoryEntry>, RepositoryError> {
        let mut response = support::checked_response(
            self.client
                .query(format!("SELECT {INTERVENTION_PROJECTION} FROM intervention WHERE vehicle = $vehicle AND id != $id AND status != 'cancelled' AND mileage IS NOT NONE AND ((service_date < $date) OR (service_date = $date AND created_at < $created_at) OR (service_date = $date AND created_at = $created_at AND id < $id)) ORDER BY service_date DESC, created_at DESC, id DESC LIMIT 1; SELECT {INTERVENTION_PROJECTION} FROM intervention WHERE vehicle = $vehicle AND id != $id AND status != 'cancelled' AND mileage IS NOT NONE AND ((service_date > $date) OR (service_date = $date AND created_at > $created_at) OR (service_date = $date AND created_at = $created_at AND id > $id)) ORDER BY service_date ASC, created_at ASC, id ASC LIMIT 1;"))
                .bind(("vehicle", support::record_id("vehicle", vehicle_id.as_str())?))
                .bind(("id", support::record_id("intervention", candidate.id.as_str())?))
                .bind(("date", midnight(candidate.service_date)))
                .bind(("created_at", candidate.created_at))
                .await,
        )?;
        let previous: Vec<DbIntervention> = support::take(&mut response, 0)?;
        let next: Vec<DbIntervention> = support::take(&mut response, 1)?;
        previous
            .into_iter()
            .chain(next)
            .map(|row| {
                row.try_into()
                    .map(|value: Intervention| value.history_entry())
            })
            .collect()
    }

    async fn transition_draft(
        &self,
        id: &InterventionId,
        target: InterventionStatus,
    ) -> Result<Intervention, RepositoryError> {
        if target == InterventionStatus::Draft {
            return Err(RepositoryError::Conflict);
        }
        let (status, timestamp) = match target {
            InterventionStatus::Completed => ("completed", "completed_at"),
            InterventionStatus::Cancelled => ("cancelled", "cancelled_at"),
            InterventionStatus::Draft => unreachable!(),
        };
        let performed_work = if target == InterventionStatus::Completed {
            "AND performed_work IS NOT NONE"
        } else {
            ""
        };
        let query = format!(
            "UPDATE ONLY $record SET status = '{status}', {timestamp} = time::now() WHERE status = 'draft' {performed_work} RETURN AFTER;"
        );
        let mut response = support::checked_response(
            self.client
                .query(query)
                .bind(("record", support::record_id("intervention", id.as_str())?))
                .await,
        )?;
        let row: Option<DbIntervention> = support::take(&mut response, 0)?;
        if let Some(row) = row {
            return row.try_into();
        }
        if self.find_by_id(id).await?.is_none() {
            Err(RepositoryError::NotFound)
        } else {
            Err(RepositoryError::Conflict)
        }
    }

    async fn mutate_line(
        &self,
        intervention_id: &InterventionId,
        mutation: LineMutation,
    ) -> Result<LineMutationResult, RepositoryError> {
        let mutation_parent = match &mutation {
            LineMutation::Create(line) | LineMutation::Update { line, .. } => {
                Some(&line.intervention_id)
            }
            LineMutation::Delete { .. } => None,
        };
        if mutation_parent.is_some_and(|id| id != intervention_id) {
            return Err(RepositoryError::Conflict);
        }
        let transaction = self
            .client
            .clone()
            .begin()
            .await
            .map_err(|error| support::classify_query_error(&error))?;
        let result = async {
            let mut parent = support::checked_response(
                transaction
                    .query("SELECT currency FROM ONLY $intervention WHERE status = 'draft';")
                    .bind((
                        "intervention",
                        support::record_id("intervention", intervention_id.as_str())?,
                    ))
                    .await,
            )?;
            let parent: Option<DbCurrency> = support::take(&mut parent, 0)?;
            let currency = CurrencyCode::parse(&parent.ok_or(RepositoryError::Conflict)?.currency)
                .map_err(|_| RepositoryError::CorruptData)?;
            let line =
                mutate_line_with_transaction(&transaction, intervention_id, mutation).await?;
            let lines = list_lines_with_client(&transaction, intervention_id).await?;
            let totals = calculate_totals(currency, &lines)?;
            Ok(LineMutationResult { line, totals })
        }
        .await;
        finish_transaction(transaction, result).await
    }

    async fn list_lines(
        &self,
        intervention_id: &InterventionId,
    ) -> Result<Vec<InterventionLine>, RepositoryError> {
        if self.find_by_id(intervention_id).await?.is_none() {
            return Err(RepositoryError::NotFound);
        }
        list_lines_with_client(&self.client, intervention_id).await
    }
}

trait QueryClient {
    fn query<'a>(&'a self, query: &'a str) -> surrealdb::method::Query<'a, Any>;
}

impl QueryClient for Surreal<Any> {
    fn query<'a>(&'a self, query: &'a str) -> surrealdb::method::Query<'a, Any> {
        Surreal::query(self, query)
    }
}

impl QueryClient for surrealdb::method::Transaction<Any> {
    fn query<'a>(&'a self, query: &'a str) -> surrealdb::method::Query<'a, Any> {
        surrealdb::method::Transaction::query(self, query)
    }
}

async fn write_intervention_query(
    client: &impl QueryClient,
    query: &str,
    record: Option<RecordId>,
    value: &NewIntervention,
) -> Result<surrealdb::IndexedResults, RepositoryError> {
    let mileage = value
        .mileage
        .map(i64::try_from)
        .transpose()
        .map_err(|_| RepositoryError::CorruptData)?;
    let mut builder = client
        .query(query)
        .bind((
            "vehicle",
            support::record_id("vehicle", value.vehicle_id.as_str())?,
        ))
        .bind(("service_date", midnight(value.service_date)))
        .bind(("mileage", mileage))
        .bind((
            "customer_reported_problem",
            value.customer_reported_problem.clone(),
        ))
        .bind(("diagnostics", value.diagnostics.clone()))
        .bind(("performed_work", value.performed_work.clone()))
        .bind(("recommendations", value.recommendations.clone()))
        .bind(("notes", value.notes.clone()))
        .bind(("currency", value.currency.as_str().to_owned()));
    if let Some(record) = record {
        builder = builder.bind(("record", record));
    }
    support::checked_response(builder.await)
}

async fn raise_vehicle_mileage(
    transaction: &surrealdb::method::Transaction<Any>,
    vehicle_id: &VehicleId,
    mileage: Option<u64>,
) -> Result<(), RepositoryError> {
    let Some(mileage) = mileage else {
        return Ok(());
    };
    let mileage = i64::try_from(mileage).map_err(|_| RepositoryError::CorruptData)?;
    support::checked_response(
        transaction
            .query("UPDATE ONLY $vehicle SET current_mileage = math::max([current_mileage ?? 0, $mileage]);")
            .bind(("vehicle", support::record_id("vehicle", vehicle_id.as_str())?))
            .bind(("mileage", mileage))
            .await,
    )?;
    Ok(())
}

async fn mutate_line_with_transaction(
    transaction: &surrealdb::method::Transaction<Any>,
    intervention_id: &InterventionId,
    mutation: LineMutation,
) -> Result<Option<InterventionLine>, RepositoryError> {
    match mutation {
        LineMutation::Create(line) => {
            let mut response = line_query(
                transaction,
                "CREATE intervention_line CONTENT { intervention: $intervention, category: $category, description: $description, quantity: $quantity_thousandths / 1000dec, unit_label: $unit_label, currency: $currency, unit_price_minor: $unit_price_minor, unit_cost_minor: $unit_cost_minor, total_price_minor: $total_price_minor, total_cost_minor: $total_cost_minor, position: $position } RETURN VALUE id;",
                None,
                &line,
            )
            .await?;
            let id: Option<RecordId> = support::take(&mut response, 0)?;
            find_line_with_client(transaction, &id.ok_or(RepositoryError::CorruptData)?)
                .await
                .map(Some)
        }
        LineMutation::Update { id, line } => {
            let mut response = line_query(
                transaction,
                "UPDATE ONLY $record MERGE { category: $category, description: $description, quantity: $quantity_thousandths / 1000dec, unit_label: $unit_label, currency: $currency, unit_price_minor: $unit_price_minor, unit_cost_minor: $unit_cost_minor, total_price_minor: $total_price_minor, total_cost_minor: $total_cost_minor, position: $position } WHERE intervention = $intervention RETURN VALUE id;",
                Some(support::record_id("intervention_line", id.as_str())?),
                &line,
            )
            .await?;
            let record: Option<RecordId> = support::take(&mut response, 0)?;
            find_line_with_client(transaction, &record.ok_or(RepositoryError::NotFound)?)
                .await
                .map(Some)
        }
        LineMutation::Delete { id } => {
            let record = support::record_id("intervention_line", id.as_str())?;
            let existing = find_line_with_client(transaction, &record).await?;
            if existing.intervention_id != *intervention_id {
                return Err(RepositoryError::NotFound);
            }
            support::checked_response(
                transaction
                    .query("DELETE ONLY $record;")
                    .bind(("record", record))
                    .await,
            )?;
            Ok(None)
        }
    }
}

async fn line_query(
    client: &impl QueryClient,
    query: &str,
    record: Option<RecordId>,
    line: &NewInterventionLine,
) -> Result<surrealdb::IndexedResults, RepositoryError> {
    let quantity =
        i64::try_from(line.quantity.thousandths()).map_err(|_| RepositoryError::CorruptData)?;
    let mut builder = client
        .query(query)
        .bind((
            "intervention",
            support::record_id("intervention", line.intervention_id.as_str())?,
        ))
        .bind(("category", category_value(line.category).to_owned()))
        .bind(("description", line.description.clone()))
        .bind(("quantity_thousandths", quantity))
        .bind(("unit_label", line.unit_label.clone()))
        .bind(("currency", line.unit_price.currency().as_str().to_owned()))
        .bind(("unit_price_minor", line.unit_price.minor_units()))
        .bind(("unit_cost_minor", line.unit_cost.map(Money::minor_units)))
        .bind(("total_price_minor", line.total_price.minor_units()))
        .bind(("total_cost_minor", line.total_cost.map(Money::minor_units)))
        .bind(("position", i64::from(line.position)));
    if let Some(record) = record {
        builder = builder.bind(("record", record));
    }
    support::checked_response(builder.await)
}

async fn list_lines_with_client(
    client: &impl QueryClient,
    intervention_id: &InterventionId,
) -> Result<Vec<InterventionLine>, RepositoryError> {
    let query = format!(
        "SELECT {LINE_PROJECTION} FROM intervention_line WHERE intervention = $intervention ORDER BY position ASC, id ASC;"
    );
    let mut response = support::checked_response(
        client
            .query(&query)
            .bind((
                "intervention",
                support::record_id("intervention", intervention_id.as_str())?,
            ))
            .await,
    )?;
    let rows: Vec<DbLine> = support::take(&mut response, 0)?;
    rows.into_iter().map(TryInto::try_into).collect()
}

async fn find_line_with_client(
    client: &impl QueryClient,
    record: &RecordId,
) -> Result<InterventionLine, RepositoryError> {
    let query = format!("SELECT {LINE_PROJECTION} FROM ONLY $record;");
    let mut response =
        support::checked_response(client.query(&query).bind(("record", record.clone())).await)?;
    let row: Option<DbLine> = support::take(&mut response, 0)?;
    row.ok_or(RepositoryError::NotFound)?.try_into()
}

async fn totals_with_client(
    client: &impl QueryClient,
    intervention_id: &InterventionId,
) -> Result<InterventionTotals, RepositoryError> {
    let mut response = support::checked_response(
        client
            .query("SELECT currency FROM ONLY $intervention;")
            .bind((
                "intervention",
                support::record_id("intervention", intervention_id.as_str())?,
            ))
            .await,
    )?;
    let currency: Option<DbCurrency> = support::take(&mut response, 0)?;
    let currency = CurrencyCode::parse(&currency.ok_or(RepositoryError::NotFound)?.currency)
        .map_err(|_| RepositoryError::CorruptData)?;
    let lines = list_lines_with_client(client, intervention_id).await?;
    calculate_totals(currency, &lines)
}

fn calculate_totals(
    currency: CurrencyCode,
    lines: &[InterventionLine],
) -> Result<InterventionTotals, RepositoryError> {
    lines
        .iter()
        .try_fold(
            InterventionTotals::zero(currency).map_err(|_| RepositoryError::CorruptData)?,
            |totals, line| totals.checked_add(line.total_price, line.total_cost),
        )
        .map_err(|_| RepositoryError::Conflict)
}

async fn finish_transaction<T>(
    transaction: surrealdb::method::Transaction<Any>,
    result: Result<T, RepositoryError>,
) -> Result<T, RepositoryError> {
    match result {
        Ok(value) => {
            transaction
                .commit()
                .await
                .map_err(|error| support::classify_query_error(&error))?;
            Ok(value)
        }
        Err(error) => {
            transaction
                .cancel()
                .await
                .map_err(|cancel| support::classify_query_error(&cancel))?;
            Err(error)
        }
    }
}

fn midnight(date: NaiveDate) -> DateTime<Utc> {
    date.and_hms_opt(0, 0, 0)
        .expect("a date always has a midnight")
        .and_utc()
}

fn status_value(status: InterventionStatus) -> &'static str {
    match status {
        InterventionStatus::Draft => "draft",
        InterventionStatus::Completed => "completed",
        InterventionStatus::Cancelled => "cancelled",
    }
}

fn parse_status(value: &str) -> Result<InterventionStatus, RepositoryError> {
    match value {
        "draft" => Ok(InterventionStatus::Draft),
        "completed" => Ok(InterventionStatus::Completed),
        "cancelled" => Ok(InterventionStatus::Cancelled),
        _ => Err(RepositoryError::CorruptData),
    }
}

fn category_value(category: InterventionLineCategory) -> &'static str {
    match category {
        InterventionLineCategory::Labour => "labour",
        InterventionLineCategory::Part => "part",
        InterventionLineCategory::Material => "material",
        InterventionLineCategory::Other => "other",
    }
}

fn parse_category(value: &str) -> Result<InterventionLineCategory, RepositoryError> {
    match value {
        "labour" => Ok(InterventionLineCategory::Labour),
        "part" => Ok(InterventionLineCategory::Part),
        "material" => Ok(InterventionLineCategory::Material),
        "other" => Ok(InterventionLineCategory::Other),
        _ => Err(RepositoryError::CorruptData),
    }
}

fn history_cursor(row: &DbIntervention) -> Result<CursorTuple, RepositoryError> {
    CursorTuple::new(
        vec![
            CursorSortValue::Date(row.service_date.date_naive()),
            CursorSortValue::Timestamp(row.created_at),
        ],
        support::record_key(&row.id, "intervention")?,
    )
    .map_err(|_| RepositoryError::CorruptData)
}

fn history_cursor_values(
    cursor: &CursorTuple,
) -> Result<(NaiveDate, DateTime<Utc>, RecordId), RepositoryError> {
    let [CursorSortValue::Date(date), CursorSortValue::Timestamp(created_at)] =
        cursor.sort_values()
    else {
        return Err(RepositoryError::CorruptData);
    };
    Ok((
        *date,
        *created_at,
        support::record_id("intervention", cursor.entity_key())?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intervention_repository_cursor_contains_all_history_sort_fields() {
        let row = DbIntervention {
            id: RecordId::new("intervention", "same_date_b"),
            vehicle: RecordId::new("vehicle", "golf"),
            service_date: midnight(NaiveDate::from_ymd_opt(2026, 7, 19).expect("valid date")),
            status: "draft".into(),
            mileage: None,
            customer_reported_problem: None,
            diagnostics: None,
            performed_work: None,
            recommendations: None,
            notes: None,
            currency: "EUR".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            cancelled_at: None,
        };
        let cursor = history_cursor(&row).expect("valid cursor");
        assert_eq!(cursor.sort_values().len(), 2);
        assert_eq!(cursor.entity_key(), "same_date_b");
    }
}
