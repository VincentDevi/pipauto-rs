# Pipauto Calendar — Product Requirements Document

## 1. Summary

Pipauto will provide an authenticated calendar that gives mechanics a clear view of their planned
interventions. The calendar is a read-only projection of interventions, not a separate appointment
or generic-event system.

The first release supports Month and Week views and must work well on phone, tablet, and desktop in
a workshop environment.

## 2. Problem

Mechanics need one place where they can quickly understand which interventions are planned and when
they are expected to happen. The intervention list does not provide a time-based view of upcoming
work.

## 3. Goal

Allow an authenticated user to see every Draft or Completed intervention overlapping a selected
month or week, using the intervention's scheduled start and estimated duration.

## 4. Product principles

- The calendar represents interventions only.
- Scheduling must preserve service-history accuracy and deterministic chronology.
- Every intervention has a meaningful start date, start time, and estimated duration.
- The identity shown for an intervention remains historically stable.
- The interface remains practical for a mechanic using a phone, tablet, or desktop.
- Completed interventions remain immutable.

## 5. Scope

### 5.1 MVP

- Add Calendar as a primary authenticated destination.
- Provide Month and Week views only, with Month as the default.
- Provide Previous, Today, Next, and view-switching navigation.
- Display every overlapping Draft and Completed intervention; exclude Cancelled interventions.
- Add a visible **New intervention** action that starts the existing active-vehicle-first workflow.
- Require all intervention creation and draft editing workflows to retain a complete schedule.
- Present duration, overlapping work, and work that crosses workshop-local midnight.
- Preserve the customer and vehicle identity captured when each intervention is created.

Calendar entries are informational in the MVP. Selecting an entry or calendar slot is not required
and must not appear as an available action.

### 5.2 Follow-up features

- Selecting a Draft intervention opens its edit form.
- Selecting a Completed intervention opens its immutable detail page.
- Selecting a Week slot starts intervention creation with its date and time prefilled.
- Selecting a Month date starts intervention creation with its date prefilled.
- Slot-based creation still requires an active vehicle and confirmed estimated duration.

### 5.3 Out of scope

- Day, agenda, list, resource, technician, or workshop-bay views.
- Generic calendar events or a separate appointment model.
- Drag-and-drop rescheduling or resize-based duration changes.
- Recurring work, reminders, customer notifications, or external calendar synchronization.
- Customer-facing booking or calendar sharing.
- Per-user or per-intervention timezones and a timezone settings interface.
- Deriving labour, costs, invoices, or completion state from estimated duration.

## 6. User stories

- As a mechanic, I want to see all planned interventions for a month so I can understand upcoming
  workload.
- As a mechanic, I want to see interventions positioned by time during a week so I can understand
  each day's schedule.
- As a mechanic, I want to distinguish Draft work from Completed work without opening it.
- As a mechanic, I want duration, overlaps, and midnight continuations to be understandable.
- As a mechanic, I want the customer and vehicle identity shown for old work to remain accurate
  after those records are edited.
- As a mechanic, I want to start the existing vehicle-first intervention workflow from Calendar.

## 7. Functional requirements

### 7.1 Calendar navigation

- The default view is Month.
- The default date is the current date in the configured workshop timezone.
- Users can switch between Month and Week views.
- Weeks run from Monday through Sunday.
- Previous and Next move by one month or week according to the active view.
- Today returns to the current workshop-local month or week.
- Week displays the complete 24-hour wall-clock day.
- The canonical address is `/calendar?view=month|week&date=YYYY-MM-DD`.
- Missing parameters select Month and today. Invalid values show a recoverable validation state.

### 7.2 Intervention visibility

- Include interventions whose lifecycle status is Draft or Completed.
- Exclude Cancelled interventions.
- Include an intervention when its scheduled interval overlaps the visible half-open range, even if
  it begins before the range.
- Load every matching intervention in the visible period without collection pagination.
- Only interventions may appear as calendar entries.

### 7.3 Entry content and historical identity

Each entry displays:

- Workshop-local start time.
- Estimated duration.
- Customer name captured when the intervention was created.
- Vehicle registration (or its recorded absence), make, and model captured when the intervention
  was created.
- Draft or Completed status in visible text.
- Continuation text where the intervention crosses a local date boundary.

Creation captures the selected vehicle's current customer identifier/name and displayed vehicle
registration/make/model. Later customer rename or reassignment and later vehicle edits do not alter
the intervention's captured values. Snapshot fields are not editable inputs.

### 7.4 Duration and overlapping work

- Week entry height represents estimated duration, not recorded labour.
- Entries whose half-open intervals overlap appear side by side when space permits.
- Adjacent entries where one ends exactly when the next starts do not overlap.
- An intervention crossing midnight continues on every affected local date.
- Month must not silently hide entries. Constrained days provide an exact complete disclosure.
- Narrow layouts may stack overlapping cards when side-by-side text would become unusable.

### 7.5 New intervention action

- Calendar includes a visible **New intervention** button.
- The action opens `/vehicles`, the existing active-vehicle-first workflow.
- The user selects an active vehicle before opening the intervention form.
- The form requires the scheduling fields defined below.

## 8. Scheduling and chronology

### 8.1 Workshop timezone

- Use a required application setting containing a valid IANA timezone.
- The initial value is `Europe/Brussels`.
- No timezone settings interface is included.
- The workshop timezone determines today, Month and Week boundaries, form interpretation, and
  presentation labels.

### 8.2 Scheduled start

- `service_date` represents the intervention's complete scheduled-start instant.
- API create/update input uses workshop-local minute precision: `YYYY-MM-DDTHH:MM`.
- Browser forms use separate required date and time controls.
- The server resolves local input through the configured timezone and stores UTC.
- Nonexistent or ambiguous local times during daylight-saving transitions are rejected with an
  actionable validation message rather than adjusted or guessed.
- Read APIs return the resolved instant in RFC 3339 UTC form.

### 8.3 Estimated duration

- `estimated_duration_minutes` is required for every intervention.
- Duration is an integer in 30-minute increments.
- Minimum duration is 30 minutes; maximum duration is 1,440 minutes.
- Duration is a scheduling estimate, not recorded labour time or an invoicing source.

### 8.4 Chronology

- Service history is ordered by complete scheduled start, creation time, and intervention ID, all
  descending.
- Mileage-neighbour validation uses the same complete ordering.
- Calendar behavior must not bypass mileage, lifecycle, line-item, or invoice rules.

## 9. Data and interface requirements

This is one mandatory, breaking scheduling contract, not an optional calendar-only extension to
date-only interventions. The schedule, duration, and identity snapshots are adopted together
before deployment so no runtime path can create or retain a partially scheduled intervention.

The intervention model requires:

- Time-aware `service_date` stored as an unambiguous UTC instant.
- Valid `estimated_duration_minutes`.
- Immutable customer identifier and display-name snapshots.
- Immutable optional vehicle registration plus required make and model snapshots.

Pipauto has no deployed workshop data. Existing disposable development/test databases are reset
and reseeded before adopting this contract. Schema rollout must refuse a data-bearing intervention
table rather than delete records or invent missing values. There is no automatic backfill: a
default time, duration, customer identity, or vehicle identity would fabricate service history.

The `/api/v1` intervention create and update contracts use workshop-local `service_date` input and
valid duration. Read responses include the UTC instant, duration, and captured identity. Existing
date-range filters remain local-date inputs and are converted to half-open UTC boundaries.

The application provides a period-bounded query returning every Draft or Completed intervention
whose calculated interval overlaps the visible Month or Week. Calendar reads do not calculate
financial totals or mutate interventions.

`GET /calendar?view=month|week&date=YYYY-MM-DD` remains a planned browser route until the issue that
owns Calendar navigation and Month rendering registers it. This contract must not be represented
by an unavailable placeholder or route that implies the calendar already works.

## 10. Responsive and accessible experience

- Month and Week are usable on phone, tablet, desktop, and at 200% text zoom.
- Controls and entries have accessible names and meaningful chronological source order.
- Keyboard users can reach and activate every navigation control.
- Status, today, selection, and continuation are not communicated by color or geometry alone.
- The page provides clear empty, loading, invalid-query, unavailable, and session-expiry states.
- Dense Month and full-day Week layouts do not conceal intervention information.
- No layout introduces page-level horizontal scrolling.
- Standard links work without JavaScript; HTMX may progressively replace only the calendar region.

## 11. Acceptance criteria

### MVP acceptance

- Month is the default view and weeks start Monday.
- Month/Week switching and Previous/Next/Today use correct workshop-local boundaries.
- Draft and Completed interventions appear; Cancelled interventions do not.
- Exact half-open overlap behavior includes work crossing the visible boundary.
- Entries show captured customer and vehicle identity, start, duration, status, and continuation.
- Later customer or vehicle changes do not rewrite captured identity.
- Duration values are limited to 30-minute increments from 30 through 1,440 minutes in browser,
  API, service, and database validation.
- DST gaps and ambiguous local input produce actionable errors.
- Mileage-neighbour validation and histories use complete timestamp ordering.
- **New intervention** starts the active-vehicle selection workflow.
- Month and Week satisfy responsive, keyboard, no-JavaScript, and accessibility requirements.

### Follow-up acceptance

- Draft entries open edit and Completed entries open immutable detail.
- Week slots prefill date/time during creation.
- Month dates prefill the date during creation.

## 12. Assumptions

- Calendar access follows the existing authenticated equal-access model.
- The initial workshop timezone is `Europe/Brussels`; configuration accepts any valid IANA zone.
- All in-repository consumers can move to the new intervention contract together before deployment.
- Disposable databases can be reset; automatic destructive migration is not authorized.
- Completed interventions remain immutable.
