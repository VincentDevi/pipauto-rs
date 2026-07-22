# Pipauto — Basic Calendar Milestone

This document is the source of truth for the Linear issues required to complete the seventh
Pipauto milestone, **Create a basic Calendar**.

It turns interventions into a workshop schedule without introducing a separate appointment or
generic-event domain. Pipauto has not been deployed, so this milestone adopts one complete
scheduling contract for every intervention. Disposable development and test databases are reset
and reseeded; the implementation must not fabricate scheduling or identity values for old rows.

## Milestone outcome

At the end of this milestone, Pipauto has an authenticated calendar that:

- Presents every overlapping Draft and Completed intervention in Month and Week views.
- Excludes Cancelled interventions and does not create a second source of workshop work.
- Opens on the current workshop-local month and supports Previous, Today, Next, Month, Week, and
  focused-day navigation through reproducible GET URLs.
- Requires a valid start date, start time, and estimated duration for every intervention.
- Stores the scheduled start as an unambiguous UTC instant after resolving workshop-local input in
  the configured IANA timezone, initially `Europe/Brussels`.
- Preserves the customer and vehicle identity displayed when an intervention was created.
- Uses the complete start timestamp for service-history chronology and mileage-neighbour checks.
- Shows duration, overlaps, and midnight continuations without hiding intervention information.
- Works as server-rendered HTML on phone, tablet, and desktop, with HTMX as optional progressive
  enhancement and no client calendar library.
- Has automated schema, domain, persistence, API, request, rendering, browser, responsive, and
  accessibility coverage.

## Out of scope

- A separate appointment, booking, availability, or generic calendar-event model.
- Selecting an entry to open its detail or edit page.
- Selecting a Month date or Week slot to start intervention creation with prefilled values.
- Day, agenda, list, resource, technician, or workshop-bay views.
- Drag-and-drop, resize-based rescheduling, recurrence, reminders, or external calendar sync.
- Customer-facing booking, notifications, email delivery, or calendar sharing.
- Recorded labour-time calculation, invoice generation, or financial calculations from estimated
  duration.
- A timezone settings page, per-user timezone, or per-intervention timezone.
- A JavaScript calendar framework, SPA state store, or client-owned chronology and layout logic.
- Automatic backfill of old intervention rows with invented start times, durations, or snapshots.

## Linear metadata

Apply the following metadata to every issue created from this document:

| Field | Value |
| --- | --- |
| Team | `VincentDevi-Perso` |
| Project | `Pipauto` |
| Milestone | `Create a basic Calendar` |
| Assignee | Unassigned |
| Cycle | None |
| Due date | None |

Create the issues in the order below and preserve their dependency relationships. Issue numbers
are dependency aliases, not final Linear identifiers. After creation, replace aliases in Linear
with actual blocking/blocked-by relationships.

This document defines issues only. Creating or modifying Linear records is a separate explicit
operation.

## Investigated calendar decision

### Existing state

Pipauto is one Loco application using Axum, Tera, HTMX, typed application services,
persistence-neutral repository contracts, SurrealDB adapters, authenticated JSON routes under
`/api/v1`, and independently mounted authenticated browser controllers. The browser application
already has desktop and phone navigation, shared form/error components, Playwright projects, and
Axe coverage.

Interventions currently use `chrono::NaiveDate` in domain and service types. SurrealDB stores the
value in a datetime field at UTC midnight, but the adapter discards the time on read. Creation and
editing accept a date-only value, repository filters and cursors use dates, and service-history
ordering is `service_date DESC, created_at DESC, id DESC`. Dashboard, vehicle history, invoices,
technical notes, API DTOs, browser forms, seeds, and tests all depend on that contract.

The application has no validated workshop timezone, estimated-duration field, customer or vehicle
identity snapshots, overlap query, calendar service, calendar presentation model, calendar route,
calendar templates, or calendar-specific CSS. The current navigation omits Calendar. The existing
vehicle list at `/vehicles` is the approved active-vehicle-first entry point for **New
intervention**.

### Selected approach

- Add `chrono-tz` as a direct dependency and add `business.workshop_timezone`, validated as an
  IANA timezone at startup. Development and test configuration use `Europe/Brussels`.
- Keep the public domain name `service_date`, but change its meaning and Rust type to a complete
  scheduled-start instant represented internally as `DateTime<Utc>`.
- Accept API create/update `service_date` as a workshop-local minute-precision string in
  `YYYY-MM-DDTHH:MM` form. Resolve it using the configured timezone. A DST gap or overlap is a
  validation error; the server never guesses an offset.
- Return `service_date` as an RFC 3339 UTC instant. Browser presentation converts it back through
  the same configured timezone. JSON date filters remain workshop-local `YYYY-MM-DD` values and
  are converted to half-open UTC boundaries before repository calls.
- Require `estimated_duration_minutes` on create. Draft patch may omit it to retain the current
  value; when supplied it is an integer from 30 through 1,440 inclusive and divisible by 30. Every
  resulting intervention remains completely scheduled. Duration is only a planning estimate.
- Snapshot the selected vehicle's customer identifier and display name plus the vehicle's displayed
  registration, make, and model during creation. These values are immutable even if the customer
  or vehicle later changes. Calendar entries use snapshots, not live joins, for displayed identity.
- Retain `vehicle_id` as the intervention relationship used for vehicle history and lifecycle
  validation. The customer snapshot ID is historical attribution, not a second mutable owner.
- Change chronology and mileage-neighbour comparisons to the complete UTC `service_date`, then
  `created_at`, then intervention ID. All three sort descending for histories and cursors.
- Query the calendar by a half-open UTC range derived from workshop-local Month or Monday–Sunday
  Week boundaries. An entry overlaps when its start is before range end and its calculated end is
  after range start.
- Because duration is at most 24 hours, persistence may use the indexable candidate bound
  `service_date >= range_start - 24 hours AND service_date < range_end`, followed by the exact
  overlap predicate. The result is period-bounded and deliberately has no collection pagination.
- Use server-owned presentation code to split entries at workshop-local midnight and assign
  deterministic overlap lanes. Tera renders validated display data; CSS performs visual placement.
- Use one full Calendar page and one replaceable calendar fragment. Every navigation control has a
  real GET URL and may add HTMX targeting and history updates.
- Reset and reseed disposable databases before applying the tightened schema. The rollout must
  fail safely if intervention rows exist; it must not delete shared data or invent replacement
  values automatically.

### Rejected alternatives

- **Preserving date-only interventions:** rejected because Pipauto has no deployed workshop data and
  one mandatory schedule is simpler and more accurate.
- **Automatic scheduling backfill:** rejected because a default time, duration, customer, or
  vehicle identity would create false history.
- **RFC 3339 input chosen by the client:** rejected because Pipauto owns one workshop timezone and
  must consistently reject ambiguous or nonexistent local input.
- **Separate appointment records:** rejected because the approved calendar is a projection of
  interventions.
- **Live customer and vehicle display joins:** rejected because later edits would rewrite the
  identity shown for historical interventions.
- **A paginated calendar query:** rejected because the visible period must not silently omit work.
- **A client-rendered calendar library:** rejected because the informational MVP does not need
  client-owned state or interaction, while the server already owns timezone and chronology rules.
- **Calendar-specific mutations:** rejected because entry and slot interaction is outside this
  milestone.

## Shared calendar contracts

### Configuration and local-time conversion

`BusinessSettings` gains a parsed workshop timezone. Invalid or missing configuration fails
startup with a setting-name error that does not echo unrelated configuration or secrets. The
timezone is application-wide and read-only in this milestone.

Input conversion follows this contract:

1. Parse the exact local `YYYY-MM-DDTHH:MM` value without accepting seconds, offsets, or partial
   dates.
2. Ask the configured IANA timezone to resolve that local value.
3. Accept exactly one matching instant.
4. Reject no matches as a nonexistent local time and two matches as an ambiguous local time, with
   actionable messages asking for another time.
5. Store and compare the resulting UTC instant.

The server derives the workshop-local current date and all Month/Week boundaries from an injected
clock in unit-tested code. Production uses the real clock. Tests do not depend on the machine's
timezone or current date.

### Intervention scheduling and snapshots

Every intervention contains:

| Field | Contract |
| --- | --- |
| `service_date` | Required UTC scheduled-start instant. Mutable only while Draft. |
| `estimated_duration_minutes` | Required integer, 30–1,440 inclusive and divisible by 30. Mutable only while Draft. |
| `customer_snapshot_id` | Required immutable customer ID captured from the selected active vehicle. |
| `customer_snapshot_name` | Required immutable trimmed displayed name captured at creation. |
| `vehicle_snapshot_registration` | Immutable optional displayed registration captured exactly; presentation uses **No registration** when absent. |
| `vehicle_snapshot_make` | Required immutable displayed make captured at creation. |
| `vehicle_snapshot_model` | Required immutable displayed model captured at creation. |

Snapshot creation and intervention insertion occur in the same service workflow and persistence
transaction. The repository rechecks that the vehicle is active and reads its current customer and
display values inside the transaction so reassignment or editing cannot produce a torn snapshot.
Update DTOs never accept snapshot fields or `vehicle_id`.

Completion and cancellation retain their current state machine. A terminal intervention cannot be
rescheduled or otherwise edited. Estimated duration does not update line quantities, labour,
costs, invoice lines, totals, or completion timestamps.

### API and browser input

The intervention create JSON contract requires:

```json
{
  "vehicle_id": "opaque-vehicle-id",
  "service_date": "2026-07-22T09:30",
  "estimated_duration_minutes": 120
}
```

Other existing optional fields retain their current meaning. Patch keeps existing optional-field
semantics: omitted `service_date` or duration retains the current value, while any supplied
scheduling value is parsed and validated. Every successful draft update validates the resulting
complete scheduling state. The API rejects snapshot fields, timezone offsets, seconds, and invalid
local times rather than silently normalizing them.

Read DTOs return the resolved UTC `service_date`, `estimated_duration_minutes`, and these immutable
objects:

```json
{
  "customer_snapshot": {
    "id": "opaque-customer-id",
    "display_name": "Mario Rossi"
  },
  "vehicle_snapshot": {
    "registration": "1-ABC-234",
    "make": "Volkswagen",
    "model": "Golf"
  }
}
```

`vehicle_snapshot.registration` is `null` when the vehicle had no registration at creation. List
and service-history date filters are inclusive local dates at the HTTP boundary and become
`[local start, day after local end)` UTC instants internally.

Browser create/edit forms use separate required `service_date`, `start_time`, and
`estimated_duration_minutes` controls. Duration choices are 30-minute increments through 24 hours.
Controllers combine the date and time, preserve all safe values after `422`, and associate error
messages with the relevant controls. The HTML contract does not expose the internal UTC value as
an editable field.

### Chronology and cursors

Service history remains deterministic:

```text
service_date DESC, created_at DESC, intervention id DESC
```

The cursor version and fingerprint must change so date-only cursors cannot be replayed against the
new timestamp contract. Cursor sort values carry the full instant without loss of precision.
Mileage-neighbour queries, model validation, SurrealDB events, dashboard ordering, vehicle history,
and invoice/technical-note intervention options all use the same chronology.

### Calendar range and entry projection

`CalendarRange` contains validated UTC `start` and `end` instants with `start < end` and the
workshop-local dates used for presentation. Only controller-created Month and Week ranges are
accepted. Arbitrary caller ranges and unbounded queries are not exposed.

`CalendarEntry` returns only what the presentation layer needs: intervention ID for stable
ordering, UTC start, duration, lifecycle status, and immutable identity snapshots. Cancelled
records never enter the result. Calendar reads do not calculate financial totals or mutate any
record.

For a requested range:

- Month begins at local midnight on the first day and ends at local midnight on the first day of
  the next month.
- Week begins at local Monday midnight and ends at the following Monday midnight.
- The exact overlap rule is `start < range_end && end > range_start`.
- End is checked with safe duration arithmetic; overflow or corrupt duration is a typed corruption
  error, never wrapped arithmetic.
- Results sort by start ascending, end ascending, status, and opaque ID before segmentation.

### Segments and Week geometry

The presentation service converts each entry to workshop-local time and splits it into one segment
per affected local date. A segment repeats start/end labels, customer, vehicle, status, and visible
continuation text. Splitting is presentation-only and never duplicates persistence rows.

Week uses 48 labelled half-hour rows as the wall-clock presentation. Start values are already on a
valid local minute and durations are in 30-minute increments. For a DST-transition day, segment
labels show the actual localized start and end, while accessible text states the elapsed duration;
the renderer must not invent or duplicate an interactive slot for a missing/repeated wall time.

For ordinary days, each segment has validated `start_minute`, `span_minutes`, `lane`, and
`lane_count` integers. The background has 48 labelled half-hour rows, while minute-based geometry
allows valid starts such as 09:15 without rounding. Overlap groups use half-open intervals so an
entry ending exactly when another starts does not overlap. Sort segments deterministically, assign
the lowest available lane, and compute the maximum simultaneous lane count for the complete
connected overlap group.

### Browser route and rendering

The public browser address is:

`GET /calendar?view=month|week&date=YYYY-MM-DD`

Missing query values select Month and today's workshop-local date. Invalid values return `422`
with **Open current month**; they are not silently replaced. Previous/Next retain the active view.
Today selects today's Month or Week. View switches retain the selected date.

Calendar is authenticated, `no-store`, and registered in both route inventories. Full-page and
HTMX responses use the same presentation model. HTMX replaces `#calendar-region`, pushes the
canonical URL, retains the previous content while busy, and restores focus through the existing
frontend behavior.

Month renders every affected date. Wide layouts show a seven-column Monday-first calendar and use
native `details` with an exact hidden count for dense days. Narrow layouts show a seven-column date
selector and the selected day's complete list. Week renders all seven days at desktop width and a
selected-day 24-hour timeline below `64rem`. No layout creates page-level horizontal scrolling.

Calendar entries are `article` content, not links or controls. Every entry visibly states start
time, captured customer, captured registration and make/model, Draft or Completed, duration, and
continuation where needed. Status and continuation never depend on color or geometry alone.

## Dependency graph

```text
Issue 1 ──→ Issue 2 ──→ Issue 3 ──┬──→ Issue 4 ──┐
                                  └──→ Issue 5 ──┴──→ Issue 6 ──→ Issue 7 ──→ Issue 8 ──→ Issue 9
```

Issue 5 may begin once the time-aware domain and repository contracts from Issue 3 are stable.
Issue 6 requires both the upgraded intervention workflows and the calendar projection. Month and
shared route work land before Week geometry. Hardening begins after both views exist.

---

## Issue 1 — Establish the scheduling and timezone contract

- **Priority:** High
- **Dependencies:** Database Migrations and Core Backend; Implement the frontend
- **Blocks:** Issue 2

### Objective

Add the validated timezone and shared scheduling foundation, and record the breaking
non-production data decision before changing persisted intervention records.

### Implementation requirements

- Add `chrono-tz` as a direct dependency using the repository's dependency convention.
- Extend `BusinessSettings` with required `workshop_timezone`, parsed to a typed IANA timezone.
- Configure development, test, and documented environment examples with `Europe/Brussels`.
- Add an injectable clock and helpers for current local date, local-to-UTC conversion, UTC-to-local
  display, Month boundaries, and Monday-first Week boundaries.
- Implement typed validation errors for malformed, nonexistent, and ambiguous local datetimes.
- Record the mandatory-scheduling, snapshot, API, reset, and no-backfill decisions in the calendar
  PRD, UI specifications, product context, and migration documentation.
- Inventory every `NaiveDate` intervention consumer and every date-only seed/fixture so subsequent
  issues cannot leave a mixed contract.
- Add the planned Calendar GET route to documentation only; do not expose a placeholder that claims
  the calendar works before its owning issue.

### Acceptance criteria

- [ ] Valid IANA zones load and invalid configuration prevents startup safely.
- [ ] `Europe/Brussels` resolves ordinary winter and summer input correctly.
- [ ] DST gaps and overlaps produce distinct actionable validation errors.
- [ ] Month and Monday-first Week boundaries are correct when UTC offsets change.
- [ ] Tests use an injected clock and do not depend on the host timezone or current date.
- [ ] Documentation contains one mandatory scheduling contract and no automatic data fabrication.

### Verification

```bash
cargo test settings
cargo test workshop_time
cargo check --all-targets
```

---

## Issue 2 — Replace the intervention schema with mandatory scheduling and snapshots

- **Priority:** High
- **Dependencies:** Issue 1
- **Blocks:** Issue 3

### Objective

Change the desired SurrealDB schema so every intervention has a complete schedule and immutable
customer and vehicle display snapshots.

### Schema and rollout requirements

- Keep `service_date` as `datetime` but treat it as an instant rather than UTC midnight standing in
  for a date.
- Add required `estimated_duration_minutes` with integer, range, and 30-minute-step assertions.
- Add required customer ID/name and vehicle make/model snapshot fields plus an optional captured
  registration field, with the approved length, normalization, and immutable-update assertions.
- Use `REFERENCE ON DELETE REJECT` for the customer snapshot relationship; snapshot display text
  remains independent of later record edits.
- Extend the terminal-update event and add snapshot immutability checks so neither draft updates nor
  direct queries can rewrite identity snapshots.
- Update service-history and recent-work indexes for the full timestamp ordering. Add the index
  required by the bounded calendar candidate query.
- Update mileage validation events to compare full timestamps and preserve creation-time/ID ties.
- Generate and inspect the reviewed rollout using the existing migration lifecycle. Its preflight
  must reject a non-empty intervention table with reset guidance; it must not delete data.
- Reset and reseed only explicitly disposable development/test databases, then update the desired
  schema snapshot, setup schema, SurrealKit suites, and fixtures.
- Prove that no startup, health, or readiness path applies schema or resets data.

### Acceptance criteria

- [ ] Clean databases require valid complete scheduling and snapshot values.
- [ ] Invalid durations, missing snapshots, and snapshot rewrites fail at the database boundary.
- [ ] Draft scheduling fields remain mutable and terminal interventions remain fully immutable.
- [ ] Calendar and chronology indexes match the reviewed desired schema.
- [ ] The rollout refuses data-bearing databases instead of inventing or deleting records.
- [ ] Reset disposable databases and clean databases converge on the same catalog snapshot.

### Verification

```bash
surrealkit test --suite 'interventions*'
cargo test migration
cargo test intervention_schema
cargo test rollout
```

---

## Issue 3 — Make intervention domain and persistence time-aware

- **Priority:** High
- **Dependencies:** Issue 2
- **Blocks:** Issues 4 and 5

### Objective

Adopt the complete schedule and captured identity throughout domain models, services, repositories,
cursors, chronology, and all non-HTTP consumers.

### Implementation requirements

- Replace intervention `NaiveDate` fields with `DateTime<Utc>` and add a validated duration value
  type that cannot represent an invalid step or range.
- Add an immutable snapshot value containing customer ID/name and vehicle registration/make/model.
- During creation, load the active vehicle and current customer, capture displayed values, and pass
  them with the schedule to one transactional repository create operation.
- Ensure the SurrealDB adapter rechecks active relationships and obtains snapshot source values in
  the same transaction as insertion.
- Extend repository projections and corruption handling for required schedule and snapshot fields.
- Version intervention cursors and serialize full timestamps without truncation.
- Update list and vehicle-history filtering to accept UTC half-open bounds derived above the
  repository boundary.
- Update mileage-neighbour model logic, repository queries, and schema behavior to use identical
  full-timestamp ordering.
- Update dashboard, vehicle history, invoice, technical-note, attachment-owner, and test-support
  consumers without allowing them to calculate or reinterpret scheduling.
- Add regression coverage for customer reassignment/rename and vehicle edits after intervention
  creation; snapshots must not change.

### Acceptance criteria

- [ ] No intervention domain or repository path truncates `service_date` to a civil date.
- [ ] Every created intervention contains valid immutable duration and identity snapshots.
- [ ] Relationship changes after creation do not change returned snapshots.
- [ ] Histories, cursors, and mileage neighbours share the complete deterministic order.
- [ ] Terminal lifecycle and line-item behavior remain unchanged.
- [ ] Corrupt/missing persisted schedule or snapshot data fails safely.

### Verification

```bash
cargo test intervention_model
cargo test intervention_service
cargo test interventions
cargo test dashboard
cargo test vehicle_history
cargo check --all-targets
```

---

## Issue 4 — Upgrade intervention API and browser forms

- **Priority:** High
- **Dependencies:** Issue 3
- **Blocks:** Issue 6

### Objective

Expose mandatory workshop-local scheduling through JSON and browser workflows while preserving
existing validation, authentication, CSRF, and lifecycle behavior.

### API and browser requirements

- Change create JSON parsing to require exact workshop-local `service_date` and valid
  `estimated_duration_minutes`. Patch may omit either to retain its current value and validates
  every supplied and resulting scheduling value.
- Return RFC 3339 UTC `service_date`, duration, and the immutable identity snapshot on intervention
  detail, list, and service-history responses.
- Keep date-range query parameters as local dates, validate `from <= to`, and convert them to UTC
  half-open bounds using the configured timezone.
- Update OpenAPI-equivalent route documentation and JSON examples in `docs/api-v1.md`.
- Replace the single browser date input with required date, time, and duration controls. Use the
  configured timezone to combine and display values.
- Preserve safe submitted values and field-linked errors after malformed input, invalid duration,
  DST gap/overlap, chronology conflict, or relationship conflict.
- Revalidate the latest vehicle/customer relationship at creation. Do not trust hidden snapshot
  values or accept them from browser input.
- Update all current request, integration, dashboard, invoice, knowledge, attachment, and browser
  fixtures that create interventions.
- Retain POST/Redirect/GET, HTMX parity, CSRF, explicit body limits, `no-store`, and session-expiry
  behavior.

### Acceptance criteria

- [ ] API and browser creation require date, time, and valid duration.
- [ ] API responses expose the resolved instant and immutable identity snapshots.
- [ ] DST errors identify the scheduling field and never silently choose an offset.
- [ ] Draft edits preserve the complete schedule and terminal edits still conflict.
- [ ] Local date filters include the complete requested workshop-local dates.
- [ ] All existing intervention creation consumers use the new contract.

### Verification

```bash
cargo test --test interventions
cargo test --test intervention_browser
cargo test --test dashboard
npx playwright test tests/browser/interventions.spec.ts
```

---

## Issue 5 — Implement the bounded calendar query and presentation engine

- **Priority:** High
- **Dependencies:** Issue 3
- **Blocks:** Issue 6

### Objective

Provide one persistence-neutral calendar read workflow and deterministic server-side presentation
engine for Month and Week views.

### Query and presentation requirements

- Add a dedicated repository method accepting only validated `CalendarRange`; do not overload the
  paginated intervention list or expose arbitrary unbounded ranges.
- Query the indexed maximum-duration candidate window, apply the exact half-open overlap predicate,
  include Draft/Completed, exclude Cancelled, and return all matches without pagination.
- Return immutable snapshots directly; avoid per-entry customer or vehicle lookups and financial
  total queries.
- Add a `CalendarService` that constructs Month/Week ranges through workshop-time helpers and maps
  repository failures to existing workflow errors.
- Build presentation-safe page, day, entry, and segment types under the view boundary.
- Split midnight-crossing entries across every affected workshop-local date with visible
  continuation-before/after labels.
- Implement deterministic interval partitioning using half-open intervals and the lowest free lane.
- Validate every numeric geometry value before serialization; templates never interpolate domain
  text into inline styles.
- Add an injected-clock test suite for current-date defaults, leap days, year boundaries, DST,
  maximum-duration lookback, exact-boundary exclusion, and corrupt duration overflow.

### Acceptance criteria

- [ ] The query returns every and only overlapping Draft/Completed intervention.
- [ ] Work starting before the visible range but ending inside it appears.
- [ ] Work ending exactly at range start and starting exactly at range end does not appear.
- [ ] Midnight continuations produce one understandable segment per affected local date.
- [ ] Adjacent entries do not overlap; connected overlap groups receive deterministic valid lanes.
- [ ] Query count does not grow through per-entry owner or totals lookups.

### Verification

```bash
cargo test calendar_range
cargo test calendar_repository
cargo test calendar_service
cargo test calendar_segments
cargo test calendar_overlap_lanes
```

---

## Issue 6 — Implement authenticated Calendar navigation and Month view

- **Priority:** High
- **Dependencies:** Issues 4 and 5
- **Blocks:** Issue 7

### Objective

Expose the authenticated Calendar route, primary navigation, and responsive Month view using the
shared calendar query and presentation engine.

### Browser requirements

- Register `GET /calendar` in browser routes, both auditable route inventories, authentication
  middleware, route tests, and frontend documentation.
- Parse only `view=month|week` and `date=YYYY-MM-DD`. Missing values select Month/today; invalid
  values produce the calendar-owned `422` recovery state.
- Add Calendar to the desktop sidebar and phone bottom bar, with correct active-navigation and
  current-area state. Keep the phone More sheet contents otherwise unchanged.
- Link **New intervention** to `/vehicles`, the existing active-vehicle-first workflow.
- Add a thin full-page template and `#calendar-region` fragment. Previous, Today, Next, Month,
  Week, and day selectors remain real GET links with optional HTMX attributes.
- Render Monday-first wide Month grids including leading/trailing days needed to complete weeks.
- Use exact native overflow disclosures containing all hidden entries in the original response.
- Below the approved breakpoint, render the seven-column date selector and complete selected-day
  entry list without horizontal page scrolling.
- Render clear empty, invalid-query, unavailable, unexpected-error, loading, and expired-session
  behavior while keeping Calendar navigation active where safe.
- Add calendar styles to the existing stylesheet using current tokens; do not add a second palette,
  stylesheet, remote asset, or calendar runtime.

### Acceptance criteria

- [ ] Calendar is authenticated, `no-store`, and active in desktop and phone navigation.
- [ ] Default, Previous, Today, Next, view-switch, focused-day, refresh, and Back URLs reproduce the
      selected state.
- [ ] Month shows every entry and continuation; dense days reveal an exact complete list.
- [ ] Entry content comes from immutable snapshots and includes textual lifecycle status.
- [ ] Standard navigation, HTMX navigation, and JavaScript-disabled navigation are equivalent.
- [ ] Phone, tablet, desktop, and 200% zoom layouts avoid page-level horizontal scrolling.

### Verification

```bash
cargo test calendar_browser
cargo test route_access_policy
cargo test browser_route_inventory
cargo test html_rendering
npx playwright test tests/browser/calendar.spec.ts --project=desktop-chromium
npx playwright test tests/browser/calendar.spec.ts --project=phone-chromium
npx playwright test tests/browser/calendar.spec.ts --project=no-javascript
```

---

## Issue 7 — Implement Week view and responsive time geometry

- **Priority:** High
- **Dependencies:** Issue 6
- **Blocks:** Issue 8

### Objective

Add the complete Monday–Sunday Week representation, duration geometry, overlap layout, and focused
narrow-screen behavior.

### Week requirements

- Render all 24 wall-clock hours and 48 half-hour rows. The time surface may scroll vertically
  inside the page, but every hour remains present and reachable.
- At desktop width, render Monday through Sunday together with synchronized time labels and day
  columns. Preserve chronological DOM order independently from visual placement.
- Position ordinary-day segments using validated start-minute, span-minute, lane, and lane-count
  values over the labelled half-hour background.
- At narrow widths, render seven normal GET day selectors and only the selected day's complete
  timeline. Retain exact counts and a visible selected-day heading.
- Stack overlapping cards at the narrowest usable width rather than shrinking, clipping, or hiding
  required identity/status text.
- Present DST-transition entries with exact localized start/end and elapsed-duration text. Do not
  create an interactive duplicated/missing slot model.
- Repeat visible identity and continuation labels on every midnight segment.
- Ensure duration height never implies recorded labour time and no Week surface becomes a slot
  creation, drag, resize, or reschedule control.

### Acceptance criteria

- [ ] Week boundaries are Monday–Sunday in the configured timezone.
- [ ] The complete 24-hour axis is available on every supported viewport.
- [ ] Duration, adjacent entries, overlaps, and connected overlap groups render deterministically.
- [ ] Midnight and DST-transition entries retain accurate labels and elapsed duration.
- [ ] Narrow layouts expose every day through standard GET links and every selected-day entry.
- [ ] Geometry never clips required identity, lifecycle, duration, or continuation content.

### Verification

```bash
cargo test calendar_week_view
cargo test calendar_overlap_lanes
npx playwright test tests/browser/calendar.spec.ts --project=desktop-chromium
npx playwright test tests/browser/calendar.spec.ts --project=tablet-chromium
npx playwright test tests/browser/calendar.spec.ts --project=phone-chromium
```

---

## Issue 8 — Harden calendar accessibility, enhancement, and failure handling

- **Priority:** High
- **Dependencies:** Issue 7
- **Blocks:** Issue 9

### Objective

Prove that both views remain complete, understandable, secure, and recoverable across input,
network, session, viewport, zoom, and assistive-technology conditions.

### Hardening requirements

- Use semantic headings, navigation, lists, articles, and `time`; do not claim ARIA grid behavior.
- Give controls and entries accessible names containing their date/time and vehicle identity.
- Keep lifecycle and continuation information visible in text and distinguish today from the
  selected day without color alone.
- Preserve chronological source order, visible focus, 44px targets, logical keyboard order,
  reduced-motion behavior, and focus recovery after HTMX replacement.
- Retain the current calendar while an HTMX request is pending, mark only the bounded region busy,
  and announce loading without blocking ordinary navigation.
- Verify escaped customer/vehicle snapshots, canonical local URLs, safe login return paths,
  `no-store`, request variation, safe correlation references, and no infrastructure leakage.
- Test empty, dense, midnight, DST, invalid-query, unavailable, unexpected-error, and expired-session
  states on representative viewports and without JavaScript.
- Run Axe against populated Month, populated Week, dense overlap, empty, invalid-query, and
  unavailable fixtures.
- Confirm no cursor, styling, role, label, or control implies entry/slot interaction.

### Acceptance criteria

- [ ] Both views are keyboard-usable, screen-reader understandable, and pass representative Axe
      checks.
- [ ] Full-page and HTMX responses retain identical records, state, authentication, and errors.
- [ ] Refresh, Back, copied URLs, and JavaScript-disabled navigation remain complete.
- [ ] Snapshot text is escaped and error responses disclose no internal data.
- [ ] Phone/tablet/desktop layouts tolerate 200% zoom without page overflow or lost records.
- [ ] No out-of-scope calendar interaction is presented as available.

### Verification

```bash
cargo test calendar
cargo test browser_security
cargo test route_access_policy
npx playwright test tests/browser/calendar.spec.ts
npx playwright test tests/browser/hardening.spec.ts
```

---

## Issue 9 — Complete milestone verification and documentation

- **Priority:** High
- **Dependencies:** Issue 8
- **Blocks:** None

### Objective

Run the complete quality gate and make product, API, migration, frontend, and operator
documentation accurately describe the implemented calendar.

### Completion requirements

- Run schema and migration suites against a clean disposable database and the explicit reset/reseed
  workflow.
- Run all domain, repository, service, API, request, rendering, route-inventory, and security tests.
- Run all Playwright projects and representative Axe checks using the disposable browser database.
- Update `docs/CONTEXT.md`, `docs/CALENDAR_PRD.md`, `docs/api-v1.md`,
  `docs/frontend.md`, `docs/ui/calendar.md`, `docs/ui/README.md`, and the calendar UI plan to match
  implemented behavior and retain the MVP exclusions.
- Document the timezone setting, local input format, UTC response format, duration contract,
  snapshots, date filters, calendar route, reset/reseed prerequisite, and troubleshooting.
- Remove statements that Calendar is deferred after it is implemented, but do not claim later
  entry/slot interactions exist.
- Confirm route listings and examples come from actual registered behavior rather than planned
  placeholders.
- Review the final diff for accidental secrets, personal workshop data, generated reports, and
  unrelated worktree changes.

### Acceptance criteria

- [ ] Clean setup and explicit disposable reset/reseed paths are documented and verified.
- [ ] Schema, Rust, request, browser, accessibility, security, and documentation checks pass.
- [ ] Product, API, frontend, and UI documents agree on mandatory scheduling and snapshots.
- [ ] Documentation distinguishes implemented MVP behavior from follow-up interactions.
- [ ] No document describes fabricated scheduling data, automatic destructive migration, or a
      separate appointment system.

### Verification

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
surrealkit test
cargo loco routes
npm ci
npx playwright test
./scripts/ci-check
```

## Milestone completion checklist

- [ ] The configured IANA workshop timezone is validated and used for every local-time boundary.
- [ ] Every intervention has a complete UTC start, valid estimated duration, and immutable customer
      and vehicle identity snapshots.
- [ ] Disposable pre-contract data is reset and reseeded; no scheduling values are fabricated.
- [ ] API and browser intervention workflows require and validate the complete schedule.
- [ ] Histories, cursors, dashboards, mileage neighbours, and dependent workflows use full
      timestamp chronology.
- [ ] The bounded overlap query includes every matching Draft/Completed intervention and excludes
      Cancelled work.
- [ ] Month and Week navigation is reproducible, Monday-first, authenticated, and `no-store`.
- [ ] Duration, overlap, midnight continuation, dense days, and DST-transition presentation are
      accurate and understandable.
- [ ] Desktop, tablet, phone, 200% zoom, keyboard, JavaScript-disabled, HTMX, and Axe coverage pass.
- [ ] No out-of-scope event, interaction, reminder, resource, or calendar framework is introduced.
- [ ] Product, API, frontend, migration, and UI documentation describes the implemented contract.
