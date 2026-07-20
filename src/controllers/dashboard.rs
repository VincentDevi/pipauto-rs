//! Authenticated workshop dashboard backed by existing intervention collection capabilities.

use axum::{http::StatusCode, response::Response};
use loco_rs::{
    controller::{
        extractor::shared_store::SharedStore, views::engines::TeraView, views::ViewEngine, Routes,
    },
    prelude::get,
    Result,
};

use crate::{
    controllers::browser::{
        context::{BrowserRequestContext, ResponsePreference},
        responses,
    },
    domain::{PageLimit, PageRequest},
    models::intervention::InterventionStatus,
    repositories::intervention::InterventionFilter,
    services::intervention::InterventionService,
    views::{
        dashboard::{DashboardPage, InterventionSection},
        layout::AuthenticatedLayout,
    },
};

const PREVIEW_LIMIT: u16 = 5;

async fn show(
    context: BrowserRequestContext,
    SharedStore(service): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
) -> Result<Response> {
    let (recent, drafts) = tokio::join!(
        service.list(preview_request(None)),
        service.list(preview_request(Some(InterventionStatus::Draft))),
    );
    let page = DashboardPage::new(
        AuthenticatedLayout::new(
            &context.current_user,
            context.csrf_token.expose(),
            &context.current_path,
        ),
        &context.current_user.display_name,
        InterventionSection::recent(recent),
        InterventionSection::drafts(drafts),
    );
    Ok(responses::render(
        context.response_preference,
        StatusCode::OK,
        page.render_page(&engine)?,
        page.render_content(&engine)?,
    ))
}

async fn recent(
    context: BrowserRequestContext,
    SharedStore(service): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
) -> Result<Response> {
    refresh_section(
        context,
        InterventionSection::recent(service.list(preview_request(None)).await),
        &engine,
    )
}

async fn drafts(
    context: BrowserRequestContext,
    SharedStore(service): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
) -> Result<Response> {
    refresh_section(
        context,
        InterventionSection::drafts(
            service
                .list(preview_request(Some(InterventionStatus::Draft)))
                .await,
        ),
        &engine,
    )
}

fn refresh_section(
    context: BrowserRequestContext,
    section: InterventionSection,
    engine: &TeraView,
) -> Result<Response> {
    if context.response_preference == ResponsePreference::FullPage {
        return Ok(responses::redirect(
            context.response_preference,
            section.dashboard_anchor(),
        ));
    }
    Ok(responses::fragment(StatusCode::OK, section.render(engine)?))
}

fn preview_request(status: Option<InterventionStatus>) -> PageRequest<InterventionFilter> {
    PageRequest {
        filter: InterventionFilter {
            status,
            ..InterventionFilter::default()
        },
        limit: PageLimit::new(PREVIEW_LIMIT).expect("dashboard preview limit is valid"),
        after: None,
    }
}

/// Routes owned by the workshop dashboard.
#[must_use]
pub fn routes() -> Routes {
    Routes::new()
        .add("/", get(show))
        .add("/dashboard/recent-interventions", get(recent))
        .add("/dashboard/draft-interventions", get(drafts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_preview_requests_are_bounded_and_filter_only_drafts() {
        let recent = preview_request(None);
        let drafts = preview_request(Some(InterventionStatus::Draft));

        assert_eq!(recent.limit.value(), PREVIEW_LIMIT);
        assert_eq!(recent.filter, InterventionFilter::default());
        assert_eq!(drafts.limit.value(), PREVIEW_LIMIT);
        assert_eq!(drafts.filter.status, Some(InterventionStatus::Draft));
        assert!(recent.after.is_none() && drafts.after.is_none());
    }
}
