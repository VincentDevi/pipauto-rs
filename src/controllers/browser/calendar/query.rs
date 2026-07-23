use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RequestedView {
    Month,
    Week,
}

impl RequestedView {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Month => "month",
            Self::Week => "week",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CalendarQuery {
    pub(super) view: RequestedView,
    pub(super) date: Option<NaiveDate>,
}

pub(super) fn parse_query(raw_query: Option<&str>) -> Result<CalendarQuery, ()> {
    let mut view = None;
    let mut date = None;
    for (key, value) in url::form_urlencoded::parse(raw_query.unwrap_or_default().as_bytes()) {
        match key.as_ref() {
            "view" if view.is_none() => {
                view = Some(match value.as_ref() {
                    "month" => RequestedView::Month,
                    "week" => RequestedView::Week,
                    _ => return Err(()),
                });
            }
            "date" if date.is_none() => {
                let value = value.as_ref();
                if value.len() != 10
                    || value.as_bytes().get(4) != Some(&b'-')
                    || value.as_bytes().get(7) != Some(&b'-')
                {
                    return Err(());
                }
                date = Some(NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| ())?);
            }
            _ => return Err(()),
        }
    }
    Ok(CalendarQuery {
        view: view.unwrap_or(RequestedView::Month),
        date,
    })
}

pub(super) fn calendar_href(view: RequestedView, date: NaiveDate) -> String {
    format!("/calendar?view={}&date={date}", view.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_query_accepts_only_reproducible_view_and_date_values() {
        assert_eq!(
            parse_query(None),
            Ok(CalendarQuery {
                view: RequestedView::Month,
                date: None,
            })
        );
        assert_eq!(
            parse_query(Some("view=week&date=2026-07-21")),
            Ok(CalendarQuery {
                view: RequestedView::Week,
                date: NaiveDate::from_ymd_opt(2026, 7, 21),
            })
        );
        for invalid in [
            "view=day",
            "date=2026-7-21",
            "date=2026-02-30",
            "date=2026-07-21&date=2026-07-22",
            "view=month&cursor=opaque",
        ] {
            assert!(parse_query(Some(invalid)).is_err(), "accepted {invalid}");
        }
    }
}
