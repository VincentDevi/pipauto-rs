//! Presentation-safe models for dashboard intervention previews and independent section states.

use loco_rs::{
    controller::views::{engines::TeraView, ViewRenderer},
    Result,
};
use serde::Serialize;

use crate::{
    domain::Page,
    models::intervention::{InterventionStatus, ServiceHistorySummary},
    services::WorkflowError,
};

use super::layout::AuthenticatedLayout;

const PAGE_TEMPLATE: &str = "pages/dashboard.html";
const CONTENT_TEMPLATE: &str = "fragments/dashboard_content.html";
const SECTION_TEMPLATE: &str = "fragments/intervention_preview.html";

#[derive(Debug, Serialize)]
pub struct DashboardPage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: &'static str,
    display_name: &'page str,
    recent: InterventionSection,
    drafts: InterventionSection,
}

#[derive(Debug, Serialize)]
pub struct InterventionSection {
    id: &'static str,
    heading_id: &'static str,
    title: &'static str,
    collection_href: &'static str,
    collection_label: &'static str,
    retry_path: &'static str,
    empty_message: &'static str,
    available: bool,
    items: Vec<InterventionPreview>,
}

#[derive(Debug, Serialize)]
struct InterventionPreview {
    href: String,
    service_date: String,
    vehicle_reference: String,
    status: &'static str,
    status_class: &'static str,
    summary: String,
    mileage: Option<String>,
}

impl<'page> DashboardPage<'page> {
    #[must_use]
    pub fn new(
        layout: AuthenticatedLayout<'page>,
        display_name: &'page str,
        recent: InterventionSection,
        drafts: InterventionSection,
    ) -> Self {
        Self {
            layout,
            title: "Dashboard · Pipauto",
            display_name,
            recent,
            drafts,
        }
    }

    /// Render the complete authenticated dashboard document.
    pub fn render_page(&self, engine: &TeraView) -> Result<String> {
        engine.render(PAGE_TEMPLATE, self)
    }

    /// Render the dashboard main region for an HTMX page refresh.
    pub fn render_content(&self, engine: &TeraView) -> Result<String> {
        engine.render(CONTENT_TEMPLATE, self)
    }
}

impl InterventionSection {
    #[must_use]
    pub fn recent(result: Result<Page<ServiceHistorySummary>, WorkflowError>) -> Self {
        Self::new(
            "recent-interventions",
            "recent-interventions-heading",
            "Recent service history",
            "/interventions",
            "View all interventions",
            "/dashboard/recent-interventions",
            "No interventions have been recorded yet. Select a vehicle to start the first intervention.",
            result,
        )
    }

    #[must_use]
    pub fn drafts(result: Result<Page<ServiceHistorySummary>, WorkflowError>) -> Self {
        Self::new(
            "draft-interventions",
            "draft-interventions-heading",
            "Draft interventions",
            "/interventions?status=draft",
            "View all drafts",
            "/dashboard/draft-interventions",
            "There are no draft interventions. Select a vehicle when new workshop work arrives.",
            result,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        id: &'static str,
        heading_id: &'static str,
        title: &'static str,
        collection_href: &'static str,
        collection_label: &'static str,
        retry_path: &'static str,
        empty_message: &'static str,
        result: Result<Page<ServiceHistorySummary>, WorkflowError>,
    ) -> Self {
        let (available, items) = match result {
            Ok(page) => (true, page.items.into_iter().map(Into::into).collect()),
            Err(_) => (false, Vec::new()),
        };
        Self {
            id,
            heading_id,
            title,
            collection_href,
            collection_label,
            retry_path,
            empty_message,
            available,
            items,
        }
    }

    #[must_use]
    pub fn dashboard_anchor(&self) -> &str {
        match self.id {
            "recent-interventions" => "/#recent-interventions",
            _ => "/#draft-interventions",
        }
    }

    /// Render one independently replaceable preview section.
    pub fn render(&self, engine: &TeraView) -> Result<String> {
        engine.render(SECTION_TEMPLATE, &SectionContext { section: self })
    }
}

#[derive(Serialize)]
struct SectionContext<'section> {
    section: &'section InterventionSection,
}

impl From<ServiceHistorySummary> for InterventionPreview {
    fn from(value: ServiceHistorySummary) -> Self {
        let intervention = value.intervention;
        let (status, status_class) = match intervention.status {
            InterventionStatus::Draft => ("Draft", "badge--warning"),
            InterventionStatus::Completed => ("Completed", "badge--success"),
            InterventionStatus::Cancelled => ("Cancelled", "badge--error"),
        };
        let summary = intervention
            .customer_reported_problem
            .or(intervention.performed_work)
            .or(intervention.diagnostics)
            .unwrap_or_else(|| "No workshop summary recorded".to_owned());
        Self {
            href: format!("/interventions/{}", intervention.id.as_str()),
            service_date: intervention.service_date.to_string(),
            vehicle_reference: format!(
                "{} · {} {}",
                intervention
                    .identity_snapshot
                    .vehicle_registration
                    .as_deref()
                    .unwrap_or("No registration"),
                intervention.identity_snapshot.vehicle_make,
                intervention.identity_snapshot.vehicle_model,
            ),
            status,
            status_class,
            summary,
            mileage: intervention.mileage.map(|value| format!("{value} km")),
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::{
        domain::{CurrencyCode, InterventionId, Money, VehicleId},
        models::intervention::{
            EstimatedDuration, Intervention, InterventionIdentitySnapshot, InterventionTotals,
        },
    };

    #[test]
    fn dashboard_sections_preserve_service_order_without_inferring_a_count() {
        let first = summary(
            "first",
            "2026-07-20T09:00:00Z",
            InterventionStatus::Completed,
        );
        let second = summary("second", "2026-07-19T09:00:00Z", InterventionStatus::Draft);
        let section = InterventionSection::recent(Ok(Page {
            items: vec![first, second],
            next_cursor: None,
        }));

        assert_eq!(section.items[0].href, "/interventions/first");
        assert_eq!(section.items[1].href, "/interventions/second");
        assert_eq!(section.collection_href, "/interventions");
    }

    #[test]
    fn dashboard_section_failure_is_bounded_and_safe() {
        let section = InterventionSection::drafts(Err(WorkflowError::Internal));

        assert!(!section.available);
        assert!(section.items.is_empty());
        assert_eq!(section.retry_path, "/dashboard/draft-interventions");
    }

    fn summary(id: &str, date: &str, status: InterventionStatus) -> ServiceHistorySummary {
        let currency = CurrencyCode::parse("EUR").expect("currency");
        ServiceHistorySummary {
            intervention: Intervention {
                id: InterventionId::parse(id).expect("id"),
                vehicle_id: VehicleId::parse("vehicle").expect("vehicle"),
                service_date: date.parse().expect("date"),
                estimated_duration: EstimatedDuration::new(60).expect("duration"),
                identity_snapshot: InterventionIdentitySnapshot::new(
                    crate::domain::CustomerId::parse("owner").expect("customer"),
                    "Owner".into(),
                    None,
                    "Volkswagen".into(),
                    "Golf".into(),
                )
                .expect("snapshot"),
                status,
                mileage: Some(120_000),
                customer_reported_problem: Some(format!("Summary {id}")),
                diagnostics: None,
                performed_work: None,
                recommendations: None,
                notes: None,
                currency,
                created_at: Utc.with_ymd_and_hms(2026, 7, 20, 12, 0, 0).unwrap(),
                updated_at: Utc.with_ymd_and_hms(2026, 7, 20, 12, 0, 0).unwrap(),
                completed_at: None,
                cancelled_at: None,
            },
            totals: InterventionTotals {
                price: Money::new(0, currency).expect("money"),
                cost: Money::new(0, currency).expect("money"),
            },
        }
    }
}
