# Pipauto — Frontend Implementation Milestone

This document is the source of truth for the Linear issues required to complete the fifth Pipauto
milestone, **Implement the frontend**.

It is based on Linear milestone `1bf35eed-ec5c-4865-8b38-e41c8bc19daf`, the UI specification under
`docs/ui/`, and the `main` branch at commit `841a5bc`. If the implementation branch has moved
forward, compare it with that commit before starting Issue 1 and update this document only when a
newly approved product, design, or architecture decision requires it.

The UI package currently consists of `docs/ui/README.md` plus the five page specifications stored
directly under `docs/ui/`. The README links to absent shared files and a non-existent `pages/`
directory. For this milestone, the README's global decisions and the five files that actually exist
are authoritative. Issue 1 records the resulting implementation contract; it must not invent pages
or active capabilities to fill those documentation gaps.

## Milestone outcome

At the end of this milestone, Pipauto has a complete authenticated, server-rendered workshop
frontend that:

- Uses Tera, plain CSS, the existing self-hosted HTMX asset, and only small purpose-specific
  JavaScript enhancements.
- Works as ordinary HTML forms and links when JavaScript or HTMX is unavailable.
- Provides the approved dashboard, customer, vehicle, intervention, technical-knowledge, invoice,
  and payment workflows.
- Calls existing application services from HTML controllers and preserves the same domain,
  validation, authorization, chronology, and financial rules as `/api/v1`.
- Uses responsive desktop, tablet, and phone layouts suitable for workshop use.
- Preserves user input after recoverable errors and presents safe, actionable error states.
- Keeps service histories in authoritative server order and all money calculations on the server.
- Has automated request, rendering, Playwright browser, and accessibility coverage.
- Documents frontend structure, assets, local development, testing, and troubleshooting.

## Out of scope

- Database migrations, schema changes, domain redesign, or new JSON API contracts.
- Binary file or image upload, preview, transformation, storage, or download.
- Invoice PDF/export generation. The UI shows explanatory unavailable text only.
- Calendar and appointment management, reminders, inventory, or parts-stock management.
- VAT, tax, jurisdiction-specific invoice wording, accounting exports, credit notes, refunds, or
  payment corrections.
- Email delivery, customer-facing portals, online payments, or contactless payments.
- Registration, password recovery, roles, permissions, multi-workshop support, or session
  management.
- Rich-text editing, client-side application state stores, a SPA framework, or client-side JSON
  rendering.
- Revenue analytics, reporting APIs, vectors, agents, or an AI mechanic assistant.

## Linear metadata

Apply the following metadata to every issue created from this document:

| Field | Value |
| --- | --- |
| Team | `VincentDevi-Perso` |
| Project | `Pipauto` |
| Milestone | `Implement the frontend` |
| Assignee | Unassigned |
| Cycle | None |
| Due date | None |

Create the issues in the order below and preserve the dependency relationships stated in each
issue. Issue numbers in this document are dependency aliases, not final Linear identifiers. After
creation, replace aliases in Linear with actual blocking/blocked-by relationships.

## Investigated frontend decision

### Existing state

Pipauto is one Loco application. It already has authenticated browser login/logout, a Tera engine,
an authenticated layout presentation type, same-origin CSS and JavaScript, self-hosted HTMX, typed
application services, SurrealDB repository adapters, and authenticated JSON APIs under `/api/v1`.
The root route still renders setup-oriented content. Business browser pages, presentation models,
templates, fragments, shared form/error components, and automated real-browser tests do not yet
exist.

The JSON controllers already translate HTTP DTOs to application services. Calling those JSON
routes from server-rendered controllers over loopback HTTP would repeat authentication and CSRF
work, create avoidable failure modes, and weaken the existing architecture. Making the browser
render JSON would turn the milestone into a client-rendered application and would not provide the
required standard-HTML fallback.

### Selected approach

The frontend remains part of the Loco monolith:

- Browser controllers parse query strings and URL-encoded forms, require `CurrentUser`, obtain the
  existing services from application state, and select a full-page, fragment, redirect, or safe
  error response.
- Browser controllers and JSON controllers share application services and domain behavior. They do
  not call each other over HTTP.
- Presentation types under `src/views` map domain/service results into template-safe display data.
  They may reuse stable API DTOs and formatting helpers, but no persistence row or SurrealDB type
  enters a view.
- Full pages live under `assets/views/pages`, reusable layouts under `assets/views/layouts`, and
  bounded HTMX responses under `assets/views/fragments`.
- Unsafe browser actions use POST, including updates and removals, so the fallback does not depend
  on method override or JavaScript. `/api/v1` retains its existing HTTP methods unchanged.
- A successful standard mutation uses POST/Redirect/GET with `303 See Other`. An equivalent HTMX
  mutation returns a bounded fragment or `HX-Redirect` when the whole-page destination changes.
- Plain CSS extends the committed palette. JavaScript enhances dialogs, focus, and the phone More
  sheet only; it does not own business state, validation, pagination, chronology, or totals.
- Playwright supplies end-to-end browser coverage. Axe runs inside representative Playwright tests.
  Both development packages are installed with exact versions and a committed npm lockfile.

### Rejected alternatives

- **Server-side loopback calls to `/api/v1`:** rejected because they duplicate transport,
  authentication, and failure handling inside one process.
- **Browser-side JSON rendering:** rejected because HTMX expects HTML fragments and the application
  must work without JavaScript.
- **A separate SPA or CSS framework:** rejected because it adds a second application architecture
  and is not required by the approved wireframes.
- **Direct repository access from HTML controllers or views:** rejected because workflow rules must
  remain in services.
- **Client-calculated financial totals or client-resorted history:** rejected because issued
  financial state and service-history chronology are authoritative server behavior.

## Shared frontend contracts

### Architecture and dependency direction

```text
browser request -> HTML controller -> application service -> repository contract
                         |                    |
                         v                    v
                 presentation model       domain models
                         |
                         v
                 Tera page or fragment
```

- Controllers own HTTP parsing, authentication extraction, CSRF form extraction, content
  negotiation, response headers, and redirect selection.
- Services own validation, relationships, lifecycle transitions, chronology, totals, and
  persistence workflows.
- Views own safe display formatting and Tera invocation. Templates select markup only.
- Templates never issue service/database calls, parse opaque IDs, calculate money, decide whether a
  lifecycle transition is legal, or trust hidden fields for authorization.
- Browser routes are registered in the auditable route-access policy and are authenticated unless
  they are an existing explicitly public or guest-only route.
- Business HTML responses retain the existing private/no-store response policy.

### Browser route inventory

The implementation may choose Rust handler names, but the browser method/path contracts are public.
Changing one requires updating `docs/ui`, route-policy tests, request tests, and Playwright tests in
the same change.

| Area | Browser routes |
| --- | --- |
| Shell | `GET /`; existing `GET/POST /login`; existing `POST /logout` |
| Customers | `GET/POST /customers`; `GET /customers/new`; `GET /customers/{id}`; `GET/POST /customers/{id}/edit`; `POST /customers/{id}/archive`; `POST /customers/{id}/restore` |
| Vehicles | `GET/POST /vehicles`; `GET /vehicles/new`; `GET /customers/{id}/vehicles/new`; `GET /vehicles/{id}`; `GET/POST /vehicles/{id}/edit`; `POST /vehicles/{id}/archive`; `POST /vehicles/{id}/restore`; `GET /vehicles/{id}/history` |
| Interventions | `GET /interventions`; `GET/POST /vehicles/{id}/interventions/new`; `GET /interventions/{id}`; `GET/POST /interventions/{id}/edit`; `POST /interventions/{id}/complete`; `POST /interventions/{id}/cancel` |
| Intervention lines | `POST /interventions/{id}/lines`; `GET/POST /interventions/{id}/lines/{line_id}/edit`; `POST /interventions/{id}/lines/{line_id}/delete`; `POST /interventions/{id}/lines/{line_id}/move-up`; `POST /interventions/{id}/lines/{line_id}/move-down` |
| Knowledge | `GET/POST /knowledge`; `GET /knowledge/new`; `GET /knowledge/{id}`; `GET/POST /knowledge/{id}/edit`; `POST /knowledge/{id}/archive`; `POST /knowledge/{id}/restore` |
| Invoices | `GET/POST /invoices`; `GET /invoices/new`; `GET /invoices/{id}`; `GET/POST /invoices/{id}/edit`; `POST /invoices/{id}/issue`; `POST /invoices/{id}/void` |
| Invoice lines | `POST /invoices/{id}/lines`; `GET/POST /invoices/{id}/lines/{line_id}/edit`; `POST /invoices/{id}/lines/{line_id}/delete`; `POST /invoices/{id}/lines/{line_id}/move-up`; `POST /invoices/{id}/lines/{line_id}/move-down` |
| Payments | `POST /invoices/{id}/payments` |
| Attachments | `POST /vehicles/{id}/attachments`; `POST /interventions/{id}/attachments`; `GET/POST /attachments/{id}/edit`; `POST /attachments/{id}/delete` |

`GET /vehicles/new` requires selecting an active customer. The nested customer route preselects its
customer. Intervention creation always starts from an active vehicle. Context parameters used to
prefill knowledge or invoice forms are local opaque IDs and must be validated by the service before
display or submission.

### Collection and navigation rules

- Customer filters are query, Active/Archived, and the configured cursor/page size.
- Vehicle filters are query, customer, registration, VIN, make, model, Active/Archived, and cursor.
- Intervention filters are vehicle, lifecycle status, service-date range, and cursor. Do not add a
  customer or free-text filter until the backend supports it.
- Knowledge filters are query, tags, make, model, engine, Active/Archived, and cursor.
- Invoice filters are lifecycle status and cursor. Do not expose customer, payment-state, number,
  or date filters until the backend supports them.
- Cursor links preserve all active typed filters. Back links preserve the originating collection
  path only when it is a validated same-origin local path.
- Pages never derive total counts when the service supplies only a page and next cursor. Render
  **Next** only from the returned `next_cursor`; use the ordinary browser Back action or an existing
  validated collection return path for earlier results. Do not synthesize reverse cursors or claim
  a page count.
- Service history uses the service result order unchanged:
  `service_date DESC, created_at DESC, id DESC`.

### Design and responsive rules

- Preserve the committed light green/neutral foundation: ink `#17211b`, page `#f4f7f5`, surface
  `#ffffff`, border `#cdd8d1`, accent/focus `#326b4b`, success `#1f6a42`, and danger `#8a1c1c`.
  Add derived tokens only after contrast verification; do not introduce a second palette.
- Use the existing system-font stack. Do not add remote fonts, icon CDNs, or third-party runtime
  assets.
- Phone layouts apply below `48rem`, tablet layouts from `48rem` through `63.999rem`, and desktop
  layouts at `64rem` and above. Layouts must also tolerate zoom and narrower component containers;
  no breakpoint may force horizontal page scrolling.
- Desktop uses the persistent sidebar. Phone uses Home, Vehicles, Jobs, and More in a bottom bar.
  More exposes Customers, Knowledge, Invoices, current display name, and Sign out.
- Desktop collections use compact tables where they fit. Phone collections use cards. Tablet uses
  the representation that avoids horizontal scrolling.
- Interactive targets are at least 44 by 44 CSS pixels except inline text links with adequate
  spacing. Visible `:focus-visible` treatment must not rely on color alone.
- Color contrast, keyboard use, focus order, labels, errors, and status announcements meet WCAG 2.2
  AA for the implemented pages. Reduced-motion preference disables non-essential transitions.
- Status, money, dates, mileage, identifiers, and destructive actions use text as well as color or
  iconography.

### Forms, HTMX, and response behavior

- Initial page loads are server-rendered. HTMX updates only the smallest stable region needed for
  a form, collection, line list, totals panel, attachment list, or notification.
- Every unsafe form includes the existing session-bound CSRF token. The same operation succeeds as
  a standard URL-encoded form and as an HTMX request.
- Submissions disable only the submitted action, change its visible label to a progress phrase, and
  prevent accidental double submission. Server state remains authoritative after an uncertain
  network result.
- `422` responses return the submitted form with a focusable summary, field-linked messages, and
  all safe entered values. Passwords remain the existing exception and are cleared.
- `409` responses explain the stale relationship/lifecycle/chronology conflict, preserve safe
  values, and provide Reload latest. They never overwrite the newer server state automatically.
- `404` renders the area-specific not-found page with a safe route back to its collection.
- Expired or revoked sessions clear the cookie and redirect to Login with a validated local return
  path. Private content is never swapped into the login page.
- `503` shows retry behavior. A mutation with an uncertain result reloads the authoritative record
  before another attempt; payments are never automatically retried.
- Unexpected errors show a correlation reference and no raw query, record representation,
  credentials, stack detail, or internal number-sequence state.
- Success notifications use an `aria-live` region and survive POST/Redirect/GET exactly once.
- Confirmation dialogs use native or equivalently accessible semantics, trap focus only while
  open, close on Escape when safe, and return focus to the opener. Without JavaScript, confirmation
  is a complete server-rendered page rather than being skipped. For a POST-only action, the first
  valid POST without an explicit confirmation value renders that confirmation page without
  mutating; the confirmed CSRF-protected POST to the same route performs the action. Crafted
  confirmation values never bypass service revalidation.

### Domain display rules

- Display money from authoritative minor units and currency through one checked formatter. Browser
  money fields parse decimal display input without floating point and pass validated minor units to
  services. The browser never calculates line, subtotal, total, paid, or outstanding values.
- Display decimal quantities from their exact string representation and retain up to three
  fractional digits.
- Display service dates as dates and persisted timestamps in the user's workshop-local
  presentation while retaining UTC values internally. Forms submit unambiguous date or local
  date-time values and controllers convert them explicitly.
- Archived records remain readable. Archive/restore controls appear only for supported entities.
- Completed/cancelled interventions and issued/void invoices remove ordinary edit controls.
- Attachment UI always says **Metadata only** and never displays a picker, camera, preview,
  thumbnail, download, checksum, uploaded-state badge, or storage claim.
- Invoice export appears as unavailable explanatory text, not an enabled control.

### Automated browser-test contract

- Add `package.json`, `package-lock.json`, and `playwright.config.*` at the repository root.
- Install `@playwright/test` and `@axe-core/playwright` with `--save-dev --save-exact`; commit the
  exact dependency versions and lockfile. Do not add a browser runtime dependency to Cargo.
- Define desktop Chromium, tablet Chromium, phone Chromium, and JavaScript-disabled projects. Use
  stable accessible roles and labels rather than CSS implementation selectors.
- Test data is isolated and deterministic. Browser tests use a disposable synchronized database,
  administrator-provisioned fixture user, and explicit cleanup; they never target personal,
  shared, staging, or production data.
- CI installs the locked npm dependencies and the required Playwright Chromium binary before the
  browser suite. Test artifacts contain no credentials, session cookies, CSRF values, or customer
  secrets.

## Dependency graph

```text
Issue 1 ──→ Issue 2 ──┬──→ Issue 3 ───────────────┐
                      ├──→ Issue 4 ──→ Issue 5 ───┤
                      ├──→ Issue 6 ──→ Issue 7 ───┤
                      ├──→ Issue 8 ───────────────┤
                      └──→ Issue 9 ──→ Issue 10 ──┤
                                                  ├──→ Issue 11 ──→ Issue 12
Issues 4 + 5 + 6 + 7 + 8 + 9 + 10 ───────────────┘
```

After Issue 2, dashboard, customer, intervention, technical-knowledge, and invoice work may proceed
in parallel subject to the relationships above. Issue 5 follows customer pages because vehicle
creation and reassignment require customer selection. Issue 7 follows the intervention detail
workflow. Issue 10 follows invoice drafting and lines. Cross-cutting hardening begins only after all
domain workflows exist.

---

## Issue 1 — Establish the frontend architecture and browser-route contract

- **Priority:** High
- **Dependencies:** Database Migrations and Core Backend; Create the wireframe of the UI, of the APP
- **Blocks:** Issue 2

### Objective

Create the shared HTML-controller, presentation, response, and browser-test foundation without
implementing business pages prematurely.

### Implementation requirements

- Add a browser-controller module boundary separate from `/api/v1` mounting while retaining the
  existing controller/service/repository dependency direction.
- Define typed extractors for URL-encoded authenticated forms with CSRF protection and configured
  body limits. JSON routes continue using their existing JSON extractor.
- Define one request-context type containing only presentation-safe current-user data, CSRF token,
  current path, validated local return path, and HTMX/full-page response preference.
- Define shared helpers for full-page rendering, fragment rendering, `303` redirects,
  `HX-Redirect`, no-store headers, validation rendering, conflict rendering, not-found pages,
  unavailable panels, and unexpected correlation references.
- Define typed form-state and field-error projections that preserve safe submitted values and map
  service `ValidationErrors` without string matching.
- Register every route in the browser route inventory in the auditable access policy. Stub routes
  may return safe `501` placeholders in this issue, but they must not appear as active navigation
  until their owning issue implements them.
- Add the npm/Playwright/Axe configuration described in the shared contract, including a minimal
  authenticated smoke test and a JavaScript-disabled login/shell smoke test.
- Document that HTML controllers call services directly. Add a structural test preventing browser
  controller/view modules from importing `repositories::surreal`, `database`, or SurrealDB types.

### Acceptance criteria

- [ ] All planned browser routes have an explicit authenticated access classification.
- [ ] `/api/v1` routes and response bodies are unchanged.
- [ ] Standard and HTMX response helpers apply equivalent authentication, CSRF, and no-store rules.
- [ ] Validation errors can be projected by field while preserving submitted values.
- [ ] Local return paths reject absolute, protocol-relative, cross-origin, and malformed values.
- [ ] No HTML controller makes a loopback HTTP request or queries a repository/database directly.
- [ ] Views and templates receive no SurrealDB row or credential/session representation.
- [ ] Exact Playwright and Axe development dependencies and the npm lockfile are committed.
- [ ] Browser smoke tests run against an isolated database and redact authentication artifacts.

### Verification

```bash
cargo check
cargo test browser_foundation
cargo test route_access_policy
npm ci
npx playwright test --project=desktop-chromium --grep @smoke
npx playwright test --project=no-javascript --grep @smoke
```

---

## Issue 2 — Implement the responsive application shell and design system

- **Priority:** High
- **Dependencies:** Issue 1
- **Blocks:** Issues 3–10

### Objective

Turn the existing authenticated layout into the reusable, responsive workshop shell and implement
the shared visual and interaction primitives required by every feature page.

### Shell and navigation

- Extend the base layout with a skip link, desktop sidebar, compact header/breadcrumb region,
  main-content landmark, notification region, and phone bottom bar.
- Desktop navigation contains Dashboard, Customers, Vehicles, Interventions, Knowledge, and
  Invoices. Phone navigation contains Home, Vehicles, Jobs, and More; More contains Customers,
  Knowledge, Invoices, the current display name, and Sign out.
- Sign out remains an authenticated CSRF-protected POST with both standard and HTMX behavior.
- Use `aria-current="page"` for the active destination. Record pages keep the owning area active.
- Make the sidebar persistent only when viewport and zoom leave adequate content width. The phone
  bottom bar must not cover form actions, validation, pagination, or the final card.

### Component system

- Convert current colors, typography, spacing, radii, borders, shadows, focus rings, and semantic
  states into documented CSS custom properties using the shared design rules.
- Implement reusable primary, secondary, quiet, and destructive buttons; fields; textareas;
  selects; checkboxes; filter bars; cards; responsive data tables; badges; definition lists;
  dialogs/sheets; empty/error/retry panels; pagination; loading indicators; and notifications.
- Components have disabled, busy, focus, error, success, and read-only states where applicable.
- Prefer semantic HTML and CSS. JavaScript is limited to open/close/focus enhancement and HTMX
  event coordination.
- Keep login and authentication-unavailable pages compatible with the revised tokens without
  changing their approved security behavior.

### Acceptance criteria

- [ ] Shell navigation matches the desktop and phone specifications at the defined breakpoints.
- [ ] Every navigation item and logout action is keyboard and touch accessible.
- [ ] All common components have a documented template/CSS contract and representative fixture.
- [ ] Tables become cards or an equivalent non-scrolling representation on narrow screens.
- [ ] Touch targets, focus visibility, reduced motion, text scaling, and color contrast meet the
      shared rules.
- [ ] The shell contains no email, session identifier, JWT, database record representation, or
      other unapproved identity data.
- [ ] Login, logout, expired-session, and authentication-unavailable behavior remains covered.
- [ ] The shell works with HTMX enabled and with JavaScript disabled.

### Verification

```bash
cargo test authenticated_shell
cargo test auth
npx playwright test --project=desktop-chromium --grep @shell
npx playwright test --project=phone-chromium --grep @shell
npx playwright test --project=no-javascript --grep @shell
```

---

## Issue 3 — Implement the workshop dashboard

- **Priority:** Medium
- **Dependencies:** Issue 2
- **Blocks:** Issue 11

### Objective

Replace the setup-oriented root page with the approved workshop dashboard using only existing
collection capabilities.

### Dashboard behavior

- Render a greeting using the presentation-safe display name and provide New intervention as the
  primary action. When no vehicle context exists, that action goes to vehicle selection.
- Provide secondary actions for New customer, Register vehicle, New invoice, and New technical
  note.
- Show limited recent intervention and draft-intervention previews using the existing intervention
  list service and server order. Link each preview to its record and complete filtered collection.
- Show an outstanding-invoice section only if existing service filters can request a correct
  outstanding set. With the current lifecycle-only filter, omit that section instead of filtering
  one arbitrary page in memory. A later backend contract may enable it.
- Do not show invented counts, revenue, profit, appointment, stock, or reporting data.
- If one preview fails, keep successful actions and other independently loaded content available;
  identify only the unavailable section and offer a bounded retry.

### Acceptance criteria

- [ ] `GET /` is an authenticated dashboard rather than the setup page.
- [ ] Quick actions reach valid browser routes and inactive features are not linked.
- [ ] Intervention previews preserve service ordering and contain no inferred total count.
- [ ] Empty data explains the next useful workshop action.
- [ ] Partial and complete service failures do not expose internal errors or remove navigation.
- [ ] Standard, HTMX refresh, tablet, and phone states match the page specification.
- [ ] No new reporting service, repository query, or JSON endpoint is introduced.

### Verification

```bash
cargo test dashboard
cargo test route_access_policy
npx playwright test --grep @dashboard
```

---

## Issue 4 — Implement customer browser workflows

- **Priority:** High
- **Dependencies:** Issue 2
- **Blocks:** Issues 5 and 11

### Objective

Allow Filippo to find, create, inspect, update, archive, and restore customers without using an API
client.

### List and detail

- Implement customer query, Active/Archived filter, configured page size, and opaque cursor
  navigation. Preserve filters in cursor and clear-filter links.
- Desktop rows and phone cards show name, available contact summary, archive state, and a detail
  link without exposing normalized lookup fields.
- Customer detail shows contact information, postal address, workshop notes, archive timestamps,
  and a paginated vehicle section using the existing customer-vehicle service capability.
- A first-customer empty state offers New customer. A no-match state retains filters and offers
  Clear filters.

### Forms and lifecycle

- Create/edit forms contain name, optional email and phone, required address line 1/postal
  code/city/country, optional address line 2, and optional workshop notes.
- Use appropriate labels, autocomplete, input modes, character limits, and country-code guidance.
  The service remains authoritative for trimming and normalization.
- Validation preserves every safe value and associates messages with the correct field.
- Archive confirmation explains that the customer leaves active lists, existing vehicles and
  history remain, and archived customers cannot receive newly assigned vehicles.
- Restore returns the record to active results. Concurrent archive/edit/restore conflicts reload
  the authoritative state.

### Acceptance criteria

- [ ] List/search/filter/cursor behavior matches the customer service contract.
- [ ] Create and edit work with standard forms and HTMX fragments.
- [ ] Submitted display values are shown without leaking normalized lookup values.
- [ ] `422`, uniqueness `409`, relationship `409`, `404`, expired-session, `503`, and unexpected
      errors render the approved states with safe input retention.
- [ ] Archive/restore is idempotent from the user's perspective and never deletes history.
- [ ] Archived customer detail is readable and offers Restore rather than ordinary mutation paths
      that violate backend rules.
- [ ] Customer vehicle navigation preserves customer context.

### Verification

```bash
cargo test customer_browser
cargo test customers_vehicles
npx playwright test --grep @customers
npx playwright test --project=no-javascript --grep @customers
```

---

## Issue 5 — Implement vehicle and service-history browser workflows

- **Priority:** High
- **Dependencies:** Issues 2 and 4
- **Blocks:** Issue 11

### Objective

Provide fast vehicle lookup, relationship-safe registration and reassignment, and direct access to
the complete authoritative service history.

### List, registration, and detail

- Implement query, customer, exact registration, exact VIN, make, model, Active/Archived, and
  cursor filters using only documented backend capabilities.
- Provide general and customer-context registration. Customer selection includes active customers
  only and does not accept an archived selection from a stale form.
- Vehicle forms contain customer, make, model, year, display registration, VIN, optional mileage,
  optional engine, and workshop notes with documented domain bounds.
- Vehicle detail shows current owner, identifying/technical fields, notes, archive state,
  metadata-only attachments, and a limited recent service-history section.
- `GET /vehicles/{id}/history` presents the complete paginated history with lifecycle/date filters
  and server ordering unchanged.

### Ownership, lifecycle, and metadata attachments

- Reassignment confirmation names the vehicle, old customer, and new customer and explains that
  service history and issued invoice snapshots remain unchanged. It does not claim ownership
  history is recorded.
- Archive confirmation explains that new interventions, invoices, and attachments cannot be added
  while archived. Existing history remains readable. Restore re-enables supported active flows.
- Attachment metadata forms contain display name, supported content type, optional non-negative
  byte size, and optional caption. Owner and `metadata_only` state are fixed.
- Metadata delete confirmation names the record. No binary controls or claims are rendered.

### Acceptance criteria

- [ ] Search and exact normalized filters use backend behavior while showing preserved display VIN
      and registration values.
- [ ] Duplicate registration/VIN, invalid VIN/year/mileage, archived-owner, stale relationship,
      and not-found errors are clear and preserve safe form state.
- [ ] Reassignment changes current ownership only and never rewrites historical records.
- [ ] Vehicle mileage updates do not rewrite intervention mileage.
- [ ] Full history is deterministic across cursor boundaries, including identical service dates.
- [ ] Cancelled interventions are visibly distinct without being removed from requested history.
- [ ] Attachment pages and fragments are explicitly metadata-only.
- [ ] All workflows operate with HTMX and standard navigation at phone, tablet, and desktop widths.

### Verification

```bash
cargo test vehicle_browser
cargo test service_history_browser
cargo test technical_notes_attachments
npx playwright test --grep @vehicles
npx playwright test --grep @service-history
```

---

## Issue 6 — Implement intervention browser workflows

- **Priority:** High
- **Dependencies:** Issue 2
- **Blocks:** Issues 7 and 11

### Objective

Implement intervention discovery, draft recording, lifecycle transitions, and immutable historical
detail while preserving mileage chronology and service-history accuracy.

### List and draft forms

- Implement vehicle, status, service-date range, and cursor filters. Do not expose unsupported
  customer or free-text controls.
- Rows/cards show service date, vehicle identity, recorded mileage, concise available narrative,
  lifecycle, authoritative price total, and detail link.
- Creation starts from an active vehicle and captures service date, optional mileage, customer-
  reported problem, diagnostics, performed work, recommendations, notes, and authoritative default
  currency.
- Edit reuses the form for Draft records only. Completion, cancellation, or an archived vehicle
  received during submission produces an authoritative conflict state rather than a silent retry.
- Backdated-mileage conflicts preserve inputs, explain the chronology rule, and link to the
  vehicle history without modifying any other intervention.

### Detail and transitions

- Draft detail shows vehicle/owner, date, mileage, narrative fields, ordered line summary, totals,
  metadata attachments, timestamps, and actions owned by Issues 7–10.
- Completion confirmation repeats vehicle, service date, recorded mileage, total, and work summary.
  Its action is **Complete and lock intervention** and explains irreversibility.
- Cancellation has no invented reason field. It explains that the record remains visible as
  Cancelled and cannot return to Draft.
- Completed/cancelled detail removes edit and line-mutation controls. Completed records may offer
  Create technical note and Create invoice draft when relationships permit; cancelled records do
  not promote invoice creation.

### Acceptance criteria

- [ ] List and vehicle history use authoritative server order and cursor semantics.
- [ ] Draft creation/editing preserves optional text and exact mileage/date values.
- [ ] Completion requires performed work and is never presented as reversible.
- [ ] Cancellation retains the intervention and its history position.
- [ ] A stale or concurrent transition reloads the authoritative state and never repeats the
      transition automatically.
- [ ] Completed/cancelled records render read-only even when reached from an old edit URL.
- [ ] `422`, chronology/state `409`, `404`, expired-session, `503`, and unexpected errors have
      tested full-page and fragment behavior.

### Verification

```bash
cargo test intervention_browser
cargo test interventions
npx playwright test --grep @interventions
npx playwright test --project=no-javascript --grep @interventions
```

---

## Issue 7 — Implement intervention lines and attachment metadata

- **Priority:** High
- **Dependencies:** Issue 6
- **Blocks:** Issue 11

### Objective

Complete the draft intervention workspace with ordered charge/cost lines and honest metadata-only
attachments.

### Line workflow

- Add/edit line forms contain category, description, positive quantity, unit label, non-negative
  unit price, optional non-negative unit cost, and stable position. Currency is displayed from the
  intervention and is not selectable.
- Parse quantity and displayed monetary input exactly without floating point. Use returned line and
  intervention totals for every render.
- Add, edit, remove, Move up, and Move down operations update only Draft interventions. Reordering
  uses explicit keyboard-accessible controls and stable positions; drag-and-drop is not required.
- Remove confirmation names the line. After every mutation, replace the line list and totals from
  the atomic service result.
- Empty lines explain that lines are optional while completion validation remains authoritative.

### Intervention attachments

- Use the shared metadata form with the intervention owner derived from the route.
- Create, edit, and delete metadata only while the existing attachment lifecycle permits it.
- Render the **Metadata only** warning in detail and forms and omit every binary-storage control.

### Acceptance criteria

- [ ] Quantity, price, cost, position, description, and category errors preserve safe inputs.
- [ ] Browser code never calculates or rounds line or intervention totals.
- [ ] Move controls produce deterministic order and remain usable without JavaScript.
- [ ] Concurrent line/state conflicts reload authoritative status, order, and totals.
- [ ] Terminal intervention pages expose no line or attachment mutation controls.
- [ ] Attachment owner/state cannot be changed through hidden or crafted form fields.
- [ ] HTMX swaps are bounded to the form, line/totals region, attachment list, and notification.

### Verification

```bash
cargo test intervention_line_browser
cargo test intervention_attachment_browser
cargo test interventions
npx playwright test --grep @intervention-lines
npx playwright test --grep @attachment-metadata
```

---

## Issue 8 — Implement technical-knowledge workflows

- **Priority:** Medium
- **Dependencies:** Issue 2
- **Blocks:** Issue 11

### Objective

Make reusable workshop knowledge quick to find, record, relate to service work, archive, and restore.

### Search and forms

- Implement full-text query, explicit tag list, make, model, engine, Active/Archived, and cursor
  filters. Search relevance and its deterministic tie-break remain server-owned.
- Results show title, safe excerpt, tags, available structured context, source indicator, updated
  date, and archive state. Do not add a client-side relevance sort.
- Create/edit forms contain required title and plain-text body, removable ordered tag chips, and
  optional make/model/engine, vehicle, and source-intervention context.
- Creating from an intervention prefills source and vehicle. Creating from a vehicle prefills the
  vehicle and available make/model/engine context. Every prefill is revalidated by the service.
- Removing a vehicle while retaining a source intervention requires resolving the inconsistency;
  the browser does not infer that the source is valid.

### Detail and lifecycle

- Render the body as safely escaped readable text, not HTML or rich text. Omit empty optional
  context sections.
- Active notes offer Edit and Archive. Archived notes remain readable and offer Restore but no
  ordinary Edit action.
- Archive confirmation explains that the note leaves default search while source relationships
  remain intact.

### Acceptance criteria

- [ ] Full-text and structured filters combine exactly as the backend defines and survive cursor
      navigation.
- [ ] Tag input preserves normalized unique order and enforces the 20-tag and per-tag limits.
- [ ] Title/body/context validation preserves all safe input.
- [ ] Source-intervention/vehicle conflicts provide correction choices and Reload latest.
- [ ] Note bodies and excerpts cannot inject markup or script.
- [ ] Active/archive empty and no-match states are distinct.
- [ ] Standard, HTMX, phone, tablet, and desktop workflows are covered.

### Verification

```bash
cargo test technical_note_browser
cargo test technical_notes_attachments
npx playwright test --grep @knowledge
npx playwright test --project=no-javascript --grep @knowledge
```

---

## Issue 9 — Implement invoice draft and line-item workflows

- **Priority:** High
- **Dependencies:** Issue 2
- **Blocks:** Issues 10 and 11

### Objective

Implement relationship-safe invoice discovery, draft creation/editing, and ordered line management
without predicting invoice numbers or owning financial calculations in the browser.

### List and draft creation

- Implement lifecycle-status and cursor filters only. Rows/cards show draft ID or issued final
  number, customer display/snapshot, available dates, lifecycle, total, paid, outstanding, and
  derived payment state supplied by the service.
- Drafts display **Draft** and an opaque internal reference only where needed for navigation; they
  never display or predict a final invoice number.
- Creation requires an active customer and authoritative default currency. Vehicle, intervention,
  and notes are optional. Prefill from customer, vehicle, or intervention context and validate the
  complete customer/vehicle/intervention relationship.
- Changing a selected customer warns before clearing incompatible vehicle/intervention selections.
  A server conflict preserves selections and requires an explicit valid choice.

### Draft detail and lines

- Draft detail shows current relationship references, currency, notes, ordered lines, subtotal,
  and total. It has no issue number/date and cannot receive payments.
- Header edit follows only fields supported by `UpdateInvoice`; due date belongs to issuance and is
  not a draft-header field.
- Line forms contain optional source-intervention-line reference, description, positive quantity,
  unit label, non-negative unit price, and position. Currency is fixed from the invoice.
- Add/edit/remove/Move up/Move down use authoritative atomic results. Source-line choices must
  belong to the related intervention when present.
- Draft voiding is owned by Issue 10; this issue exposes no terminal behavior beyond a placeholder
  action hidden until Issue 10 is complete.

### Acceptance criteria

- [ ] Invoice list exposes only supported filters and preserves opaque cursor state.
- [ ] Draft creation rejects archived customers and inconsistent relationships without losing safe
      form data.
- [ ] No draft predicts or reserves a final number.
- [ ] Header and line forms exactly match existing service commands.
- [ ] Currency, line totals, subtotal, and total are always service results.
- [ ] Source-line, relationship, currency, immutable-state, and concurrent-total conflicts reload
      authoritative values.
- [ ] Reordering is deterministic, keyboard accessible, and usable without JavaScript.

### Verification

```bash
cargo test invoice_draft_browser
cargo test invoice_line_browser
cargo test invoices
npx playwright test --grep @invoice-drafts
npx playwright test --project=no-javascript --grep @invoice-drafts
```

---

## Issue 10 — Implement invoice lifecycle and payment workflows

- **Priority:** High
- **Dependencies:** Issue 9
- **Blocks:** Issue 11

### Objective

Implement invoice issuance, immutable issued/void presentation, eligibility-aware voiding, and
append-only payment recording.

### Issuance and immutable detail

- Issue confirmation repeats customer, related vehicle/intervention, line count, authoritative
  total, and due date. It captures required issue date and optional due date.
- The action is **Issue and lock invoice** and explains that numbering, snapshots, and lines become
  immutable and cannot return to Draft.
- Empty drafts cannot open or submit issuance. A stale/concurrent response reloads lifecycle and
  totals and never repeats number allocation.
- Issued detail shows final number, issue/due dates, customer/billing snapshots, immutable lines,
  totals, paid, outstanding, derived payment state, relationships, and ordered payments.
- Fully paid and void invoices remove Record payment. Issued/void pages expose no header or line
  edit controls.

### Payments and voiding

- Payment form contains positive amount, required received date/time, Cash/Bank transfer/Card/Other
  method, optional reference, and optional notes. Currency is fixed from the invoice.
- Explain that payments are append-only and cannot be edited or deleted. No such controls appear.
- On success, refresh payments and every derived amount/status from the returned authoritative
  invoice. On uncertain `503`, reload before enabling another attempt.
- An overpayment/concurrent-payment conflict displays the new outstanding balance and requires an
  explicit corrected resubmission.
- Void is offered only when the current service state permits it. Confirmation requires the
  bounded reason and explains that the invoice remains in records and cannot receive payments.
- Show invoice export as unavailable explanatory text. Do not render an enabled export button.

### Acceptance criteria

- [ ] Issuance allocates and displays the final number once and never predicts or reuses one.
- [ ] Issued snapshots and lines are read-only in every browser path.
- [ ] Unpaid, partially paid, and paid are derived from authoritative payment data.
- [ ] Payments cannot exceed the latest outstanding balance and are never retried automatically.
- [ ] Payment rows have no edit/delete affordance and contain safe attribution/display data only.
- [ ] An invoice with a payment cannot be voided; stale eligibility reloads current state.
- [ ] Void records retain their number, timestamp, reason, and history.
- [ ] Tax, legal-compliance, email, provider, refund, correction, and export claims are absent.

### Verification

```bash
cargo test invoice_lifecycle_browser
cargo test payment_browser
cargo test invoices
npx playwright test --grep @invoice-lifecycle
npx playwright test --grep @payments
```

---

## Issue 11 — Harden progressive enhancement, accessibility, and responsive behavior

- **Priority:** High
- **Dependencies:** Issues 3–10
- **Blocks:** Issue 12

### Objective

Audit and harden the complete frontend as one coherent application across HTMX, standard HTML,
keyboard input, assistive technology, zoom, and supported viewport classes.

### Progressive-enhancement audit

- Exercise every unsafe action with HTMX and with JavaScript disabled. Both paths must enforce the
  same authentication, CSRF, validation, relationship, lifecycle, and error rules.
- Verify fragments have one stable replacement target, cannot nest full documents, update document
  title/history when required, and restore focus to the changed form or initiating control.
- Verify busy states prevent accidental duplicates without permanently disabling recovery after a
  network or server failure.
- Verify expired sessions from full-page and HTMX requests clear the stale session and reach Login
  without inserting private HTML into a public page.
- Verify uncertain mutations reload authoritative state before retry, especially completion,
  issuance, voiding, and payment.

### Accessibility and responsive audit

- Test landmarks, heading order, page titles, labels, descriptions, error associations, tables,
  cards, dialogs/sheets, notifications, and status badges with Axe and keyboard inspection.
- Verify skip link, visible focus, logical focus order, Escape/close behavior, focus return, live
  announcements, reduced motion, 200% zoom, 44px touch targets, and text/non-text contrast.
- Run principal flows in desktop, tablet, and phone Playwright projects. Verify no horizontal page
  scroll, covered final content, inaccessible hover-only action, or lost record context.
- Verify user-authored customer, vehicle, intervention, technical-note, attachment, invoice, and
  payment text is escaped in pages, fragments, attributes, and notifications.

### Acceptance criteria

- [ ] Every mutation has automated HTMX and JavaScript-disabled coverage or a documented reason it
      has no enhanced behavior.
- [ ] Representative pages and dialogs pass automated Axe checks with no serious or critical
      violations.
- [ ] Principal workflows are keyboard-completable and preserve visible focus.
- [ ] Desktop, tablet, phone, text zoom, and reduced-motion checks meet the shared design rules.
- [ ] Authentication/CSRF/no-store behavior is equivalent across page and fragment responses.
- [ ] User-authored text is safely escaped and internal errors are absent from rendered output.
- [ ] Deferred features do not appear as active controls.

### Verification

```bash
cargo test browser_security
cargo test html_rendering
npx playwright test --project=desktop-chromium
npx playwright test --project=tablet-chromium
npx playwright test --project=phone-chromium
npx playwright test --project=no-javascript
```

---

## Issue 12 — Complete milestone verification and frontend documentation

- **Priority:** Medium
- **Dependencies:** Issue 11
- **Blocks:** None

### Objective

Prove the complete frontend from a clean checkout and leave implementation, testing, and recovery
documentation sufficient for later calendar and image-storage milestones.

### Verification scenario

From a disposable database, apply the documented schema workflow, provision the fixture user, and
exercise this sequence through the browser:

1. Sign in and use desktop and phone navigation.
2. Create and edit a customer, register a vehicle, and verify customer-to-vehicle navigation.
3. Create a draft intervention, add/reorder lines and attachment metadata, handle a validation
   error, complete it, and verify immutable chronological history.
4. Create a technical note from the intervention, find it through full-text/structured filters,
   archive it, and restore it.
5. Create an invoice draft from the completed intervention, add/reorder lines, issue it, record
   partial and final payments, and verify immutable snapshots and derived Paid state.
6. Exercise representative conflict, not-found, unavailable, expired-session, empty, and no-match
   states.
7. Repeat principal unsafe paths with JavaScript disabled.

### Documentation deliverables

- Update `README.md` with locked npm installation, Playwright browser installation, frontend run,
  browser test, and troubleshooting commands.
- Update `docs/architecture.md` with the HTML-controller/presentation dependency direction and the
  explicit no-loopback decision.
- Add frontend documentation covering browser routes, template/fragment organization, design
  tokens, component contracts, HTMX conventions, focus behavior, test fixtures, and adding a page.
- Update the route/API documentation only to distinguish browser HTML routes from unchanged
  `/api/v1` JSON routes.
- Document that the current `docs/ui/README.md` links to absent shared files and identify the six
  actual UI documents used by this milestone; do not fabricate approval history.
- Extend CI to use `npm ci`, install the pinned Chromium browser/dependencies, and run Playwright and
  Axe without targeting preserved databases.

### Acceptance criteria

- [ ] A clean checkout can install locked Rust/npm dependencies, initialize disposable data, start
      Pipauto, and run all frontend tests using documented commands.
- [ ] The complete principal workflow succeeds on desktop and phone and remains functional without
      JavaScript.
- [ ] Route protection, CSRF, no-store behavior, safe redirects, escaping, and correlation-error
      behavior are verified.
- [ ] History chronology, terminal-state immutability, server totals, issued snapshots, and payment
      balances are verified through the rendered UI.
- [ ] Accessibility and responsive acceptance results are reproducible.
- [ ] Browser artifacts and logs contain no password, JWT, session cookie, CSRF value, database
      credential, or private customer fixture beyond explicitly synthetic test data.
- [ ] Documentation describes all browser routes, components, assets, tests, and known deferred
      capabilities.
- [ ] No tracked file contains a secret or secret-shaped fixture.

### Final verification

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo loco routes
npm ci
npx playwright install --with-deps chromium
npx playwright test
./scripts/ci-check
```
