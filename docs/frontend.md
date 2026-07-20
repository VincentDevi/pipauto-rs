# Pipauto frontend guide

Pipauto's first-release frontend is server-rendered HTML with progressive HTMX enhancement. Every
workshop workflow remains usable as ordinary links and URL-encoded forms when JavaScript is
disabled. Calendar screens and binary image storage remain deferred; attachment controls describe
metadata only.

## Runtime boundary

Browser controllers live in `src/controllers/browser` (with authentication, dashboard, and setup
controllers composed alongside them). They parse browser input, call application services
directly, map results into types under `src/views`, and select a complete page or fragment. They
never call Pipauto's `/api/v1` JSON routes over loopback HTTP.

All HTML routes pass through the browser `no-store` layer. Except for guest-only login, routes are
authenticated on the server. Unsafe forms use `application/x-www-form-urlencoded`, include the
session-bound `_csrf` value, and have an explicit body limit. HTMX may also send the same token in
`X-CSRF-Token`; it does not weaken origin, session, expiry, or action binding. Browser views receive
presentation-safe values only, never persistence rows, credentials, JWTs, session records, or raw
infrastructure errors.

## Browser route inventory

`src/controllers/browser/mod.rs::ROUTE_INVENTORY` is the executable source of truth. Its unit test
must match every registered browser method and path. `cargo loco routes` prints both this HTML
surface and the separately mounted `/api/v1` JSON surface.

| Area | Read routes | Unsafe routes |
| --- | --- | --- |
| Session | `GET /login`, `GET /` | `POST /login`, `POST /logout` |
| Dashboard/setup | `GET /dashboard/recent-interventions`, `GET /dashboard/draft-interventions`, `GET /setup/status` | — |
| Customers | `GET /customers`, `/customers/new`, `/customers/{id}`, `/customers/{id}/edit` | `POST /customers`, `/customers/{id}/edit`, `/customers/{id}/archive`, `/customers/{id}/restore` |
| Vehicles | `GET /vehicles`, `/vehicles/new`, `/customers/{id}/vehicles/new`, `/vehicles/{id}`, `/vehicles/{id}/edit`, `/vehicles/{id}/reassign`, `/vehicles/{id}/history` | `POST /vehicles`, `/vehicles/{id}/edit`, `/vehicles/{id}/reassign`, `/vehicles/{id}/archive`, `/vehicles/{id}/restore` |
| Vehicle attachments | `GET /vehicles/{id}/attachments/new`, `/attachments/{id}/edit` | `POST /vehicles/{id}/attachments`, `/attachments/{id}/edit`, `/attachments/{id}/delete` |
| Interventions | `GET /interventions`, `/vehicles/{id}/interventions/new`, `/interventions/{id}`, `/interventions/{id}/edit`, `/interventions/{id}/complete`, `/interventions/{id}/cancel` | `POST /vehicles/{id}/interventions`, `/interventions/{id}/edit`, `/interventions/{id}/complete`, `/interventions/{id}/cancel` |
| Intervention lines | `GET /interventions/{id}/lines/new`, `/interventions/{id}/lines/{line_id}/edit` | `POST /interventions/{id}/lines`, `/interventions/{id}/lines/{line_id}/edit`, `/interventions/{id}/lines/{line_id}/delete`, `/interventions/{id}/lines/{line_id}/move-up`, `/interventions/{id}/lines/{line_id}/move-down` |
| Intervention attachments | `GET /interventions/{id}/attachments/new`, `/interventions/{id}/attachments/{attachment_id}/edit` | `POST /interventions/{id}/attachments`, `/interventions/{id}/attachments/{attachment_id}/edit`, `/interventions/{id}/attachments/{attachment_id}/delete` |
| Technical knowledge | `GET /knowledge`, `/knowledge/new`, `/knowledge/{id}`, `/knowledge/{id}/edit` | `POST /knowledge`, `/knowledge/{id}/edit`, `/knowledge/{id}/archive`, `/knowledge/{id}/restore` |
| Invoices | `GET /invoices`, `/invoices/new`, `/invoices/{id}`, `/invoices/{id}/edit`, `/invoices/{id}/issue`, `/invoices/{id}/void`, `/invoices/{id}/payments/new` | `POST /invoices`, `/invoices/{id}/edit`, `/invoices/{id}/issue`, `/invoices/{id}/void`, `/invoices/{id}/payments` |
| Invoice lines | `GET /invoices/{id}/lines/new`, `/invoices/{id}/lines/{line_id}/edit` | `POST /invoices/{id}/lines`, `/invoices/{id}/lines/{line_id}/edit`, `/invoices/{id}/lines/{line_id}/delete`, `/invoices/{id}/lines/{line_id}/move-up`, `/invoices/{id}/lines/{line_id}/move-down` |

Parameterized identifiers are opaque and links must use server-provided values. GET routes must not
mutate state. Every unsafe path needs a normal form submission path; JavaScript-only mutations are
not accepted.

## Templates and fragments

The Tera tree has four roles:

- `assets/views/layouts/base.html` owns document structure, skip link, desktop sidebar, phone
  navigation, notification regions, CSRF metadata, and self-hosted assets.
- `assets/views/pages/*.html` extends the base layout and renders a complete response. A page
  normally includes its matching fragment so full-page and HTMX representations cannot drift.
- `assets/views/fragments/*.html` owns replaceable workflow regions: lists, details, forms, line
  regions, transition confirmations, errors, and unavailable states.
- `assets/views/fixtures/components.html` is a development fixture for representative component
  states; it is not a production route or test-data source.

`assets/static/css/app.css` is the single first-release stylesheet. `assets/static/js/app.js`
contains optional progressive behavior. `assets/static/vendor/htmx.min.js` is the pinned,
self-hosted HTMX runtime; no page depends on a CDN.

Templates escape values by default. Use Tera's `safe` filter only for server-built local URLs or
already-rendered trusted markup whose construction is covered by tests. Never mark customer,
vehicle, intervention, note, invoice, attachment, or error text as safe.

## Design tokens

Global custom properties are declared at the top of `app.css` and are the contract for new styles:

- Color: `--color-ink`, `--color-muted`, canvas/surface/border tokens, and brand, danger,
  success, and warning pairs.
- Type: `--font-sans`, `--font-size-sm` through `--font-size-xl`, and `--line-height`.
- Spacing: `--space-1` through `--space-8`.
- Shape/elevation: radius, border, card shadow, and sheet shadow tokens.
- Interaction/layout: `--focus-ring`, `--target-size`, `--sidebar-width`, and
  `--phone-bar-height`.

Use tokens before adding literal values. Interactive targets must retain the target-size and focus
contracts. Collection tables collapse into labelled record cards where the stylesheet's responsive
rules require it; a new page must not introduce horizontal page scrolling at phone widths or 200%
text zoom.

## Component contracts

| Component | Required behavior |
| --- | --- |
| Base shell | One `main#main-content`, working skip link, correct `aria-current`, desktop sidebar, phone bottom navigation and More sheet, safe logout form. |
| Collection | Labelled filters submitted by GET, validated query values, deterministic pagination links, populated and empty/no-match states, table/card responsive representation. |
| Detail | Server-authoritative lifecycle badge, stable resource relationships, direct next actions, explicit immutable/archived restrictions. |
| Form | Visible labels, preserved values after validation/conflict, field errors plus focusable summary, hidden `_csrf`, explicit normal `method` and `action`. |
| Line region | Server-calculated totals, deterministic positions, POST move controls, and mutation controls only while the parent is a draft. |
| Transition confirmation | GET review followed by POST confirmation; describe irreversibility and render the refreshed authoritative state after a conflict. |
| Attachment metadata | State plainly that no binary exists, expose no file input/upload/download control, keep the owner fixed, and show `metadata_only`. |
| Notification/error | Use a live region or `role="alert"`; expose safe recovery guidance and correlation ID only for unexpected/unavailable failures. |
| Unavailable state | Keep the owning navigation area active, explain that records are unchanged, and provide a safe retry or return path. |

Empty means no records exist. No-match means records may exist but filters excluded them. Do not
merge these states. Not-found, conflict, expired-session, unavailable, and unexpected-error
responses likewise have distinct recovery behavior.

## HTMX conventions

- Every `hx-get` has a real `href` or GET form action; every `hx-post` has a real POST form action.
- Target the smallest stable region that represents the authoritative result. Use
  `#main-content` when the workflow changes page context.
- A fragment response includes its own target root. Full-page and fragment rendering use the same
  presentation model and status semantics. Responses vary on `HX-Request` where their body differs.
- Search/filter and pagination requests use `hx-push-url="true"` so refresh, back, and copied URLs
  reproduce the state.
- Unsafe requests include `_csrf`. `app.js` mirrors it into the HTMX header only for same-origin
  unsafe requests.
- Expected `409` and `422` fragments are swappable and retain entered values. Authentication
  expiry navigates to `/login?next=<validated-local-path>`; external or malformed return paths are
  rejected.
- Disable the initiating control while a request is running, mark the target busy, and restore the
  control afterward. A transport failure announces that the latest workshop record must be
  reloaded before retrying an uncertain mutation.

Do not add custom JavaScript for behavior that native HTML or an HTMX attribute already expresses.
Any enhancement must leave the standard request path complete.

## Focus behavior

Native full-page navigation uses document order and the skip link. On an HTMX swap, `app.js` applies
this priority:

1. Focus the first field marked `aria-invalid="true"`.
2. Otherwise restore the initiating control when an equivalent control exists in the replaced
   region.
3. Otherwise focus the swapped region (adding `tabindex="-1"` when needed).

Dialog close returns focus to its opener. Escape closes the phone More sheet and returns focus to
its summary. Do not add `autofocus` to an error response: preserved invalid input or the error
summary owns focus. Dynamic status text belongs in the existing polite/assertive live regions.

## Browser tests and fixtures

Install and run the locked toolchain with:

```bash
npm ci
npx playwright install --with-deps chromium
npx playwright test
```

`playwright.config.ts` runs one worker because every project shares one deliberately disposable
database. The projects are desktop Chromium, tablet Chromium, JavaScript-disabled desktop
Chromium, and phone Chromium. The specs under `tests/browser` cover:

- login, protected shell, desktop/phone navigation, skip link, and no leaked authentication
  artifacts;
- customer creation/editing and validation, vehicle registration/navigation, metadata, archive,
  restore, and service-history states;
- intervention draft editing, line ordering, authoritative totals, metadata, completion,
  chronology, and terminal immutability;
- technical-note creation from vehicle/intervention context, escaping, full-text plus structured
  filters, no-match, archive, and restore;
- invoice drafts and line ordering, issuance snapshots, immutability, partial/final payments,
  outstanding-balance conflict, derived Paid state, and retained void history;
- empty, no-match, not-found/unavailable/request failures, expired session, responsive layout,
  focus recovery, reduced motion, and Axe checks.

`scripts/browser-smoke-server` owns the fixture lifecycle. It creates only the synthetic
`browser-smoke@example.invalid` user and synthetic workshop records, runs the documented schema
sync and dry run against `pipauto_browser/browser_smoke`, and destroys the Compose volume on exit.
Playwright's global teardown independently removes that same disposable project so forced
web-server process termination cannot leave port `18000` or the test volume behind.
The password is a synthetic local fixture, never a real account secret. Do not add private customer
data or reuse a preserved database.

Playwright screenshots, video, and traces are disabled. Keep those defaults unless a separate,
reviewed redaction and retention decision is made. Test output, uploaded artifacts, and application
logs must not contain passwords, JWTs, session cookies, CSRF values, database credentials, hashes,
or non-synthetic customer data. The Rust gate also scans tracked documentation and fixtures for
private-key, JWT, and Argon2-shaped values.

## Adding a page

1. Confirm the workflow is in the initial-release scope and use the vocabulary in
   `documentations/CONTEXT.md`. Do not turn calendar, upload/storage, export, or another deferred
   capability into an active control.
2. Add or extend a service workflow and presentation model. Keep database and API DTO types out of
   the view boundary.
3. Add the controller route, classify it in `ROUTE_INVENTORY`, apply authentication/body-limit
   conventions, and map service outcomes to the established HTML states.
4. Add a fragment and a thin full-page template that includes it. Provide ordinary links/forms
   first, then optional HTMX attributes targeting a stable root.
5. Reuse design tokens and component classes. Check keyboard order, focus after validation and
   swaps, live announcements, touch targets, phone layout, 200% zoom, and reduced motion.
6. Add request tests for route protection, CSRF, `no-store`, safe redirects, escaping, response
   status/correlation behavior, and business invariants. Add Playwright coverage to every relevant
   viewport and the JavaScript-disabled project; add Axe assertions for representative states.
7. Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
   `cargo loco routes`, and the locked browser commands above. Update this route/component guide and
   the product context in the same change only when an approved product decision changed.
