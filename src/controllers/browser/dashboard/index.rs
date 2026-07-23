use super::*;

const PREVIEW_LIMIT: u16 = 5;

pub(super) async fn show(
    context: BrowserRequestContext,
    SharedStore(service): SharedStore<InterventionService>,
    ViewEngine(engine): ViewEngine<TeraView>,
) -> Result<Response> {
    let (recent, drafts) = tokio::join!(
        service.list(preview_request(None)),
        service.list(preview_request(Some(InterventionStatus::Draft))),
    );
    let page = DashboardPage::new(
        context.layout(),
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

pub(super) async fn recent(
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

pub(super) async fn drafts(
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

pub(super) fn refresh_section(
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

pub(super) fn preview_request(
    status: Option<InterventionStatus>,
) -> PageRequest<InterventionFilter> {
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
