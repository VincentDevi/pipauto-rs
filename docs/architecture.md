# Pipauto architecture

Pipauto is a single Loco application serving HTTP directly. Shared domain, persistence, service,
and API contracts keep later business areas consistent without coupling them to Loco or SurrealDB.

## Dependency direction

```text
JSON controllers → API DTOs
        ↓
      services → domain ← repository contracts
        ↑                         ↑
HTML controllers          SurrealDB adapters
        ↓
presentation models → views/templates
app/initializers → compose all infrastructure
```

Dependencies point inward toward workflows, persistence contracts, and database-independent domain
models. Composition code is the exception: `app` and `initializers` know concrete infrastructure so
the rest of the application does not need to.

## Module boundaries

| Module | Owns | Must not own |
| --- | --- | --- |
| `app` | Route, initializer, middleware, and shared-service composition | Business rules or persistence behavior |
| `controllers` | HTTP input parsing and response selection | Business rules or database queries |
| `domain` | Shared IDs, money, quantity, normalization, archive chronology, validation, and pagination invariants | Loco, Axum, Tera, SurrealDB, HTTP query strings, or row structs |
| `models` | Database-independent authentication, customer, vehicle, intervention, line, technical-note, attachment, invoice, and payment models | Loco, Axum, Tera, or SurrealDB concerns |
| `api` | Explicit IDs, timestamps, money, quantity, pagination, and error DTOs | SurrealDB rows, repository errors, or business decisions |
| `services` | Application workflows across models and repository contracts | HTTP, templates, or concrete databases |
| `repositories` | Persistence-neutral record and attachment-file contracts, errors, typed domain filters, and cursors | HTTP query strings, SurrealDB types, templates, or workflow policy |
| `repositories::surreal` | SurrealDB adapters and centralized record-ID, response, query-error, and cursor-tuple mechanics | HTTP or template behavior |
| `database` | Settings, connection, authentication, database selection, and health checks | Domain persistence contracts or workflows |
| `initializers` | Loco lifecycle wiring and shared-store registration | Business workflows or HTTP behavior |
| `views` | Typed presentation data and Tera rendering | HTTP parsing, business rules, or persistence |
| `settings` | Validated business defaults and collection bounds | Secrets or feature-specific workflow policy |
| `errors` | Workflow-to-HTTP status and safe error-envelope mapping | Repository errors or raw infrastructure details in client responses |

Business-domain repository contracts live in `repositories` when their workflows are defined.
Absence uses `Option`; conditional mutation absence may use `RepositoryError::NotFound`.
`Unavailable` and `CorruptData` remain distinct repository failures and are never converted to
not-found. Services translate repository results to `WorkflowError`; controllers alone translate
workflow outcomes into HTTP statuses and `api::ErrorEnvelope` values.

Collection contracts take `PageRequest<F>` with a typed `CollectionFilter`, `PageLimit`, and
`OpaqueCursor`. Cursor signatures bind a version, API resource kind, deterministic final sort
tuple, and every filter affecting membership or order. A purpose-separated HMAC key derived from
the CSRF secret authenticates cursors without reusing the raw JWT or CSRF key. Cursor entity keys
exclude SurrealDB's serialized `table:id` representation. SurrealDB rows stay private to adapters
and are explicitly converted into domain models before DTO conversion.

Business controllers contribute ordinary Loco routes through `controllers::api_v1`, which applies
the `/api/v1` prefix and `no-store` response policy. Every handler explicitly extracts
`CurrentUser`; unsafe JSON handlers additionally use `AuthenticatedCsrfJson<T>`, with a per-route
`DefaultBodyLimit`. Controllers parse DTOs and select responses only.

Server-rendered controllers are composed separately through `controllers::browser`. HTML
controllers call application services directly; they do not call the JSON API over loopback HTTP.
Their shared request context contains only a display-safe user, CSRF token, current path, validated
local return path, and full-page/HTMX preference. URL-encoded unsafe forms use the typed
`AuthenticatedForm<T>` extractor and an explicit body limit; JSON routes keep their existing JSON
extractor. Browser views receive typed presentation models, never database rows, credentials, or
session records.

This no-loopback rule is explicit: `/api/v1` is a sibling delivery adapter, not an internal client
boundary. HTML controllers may share services, domain types, and repository contracts with JSON
controllers, but must not send HTTP requests to Pipauto itself or deserialize API DTOs to render a
page. Mapping flows one way from service results into presentation models and then templates;
templates and presentation models do not depend on controllers, API DTOs, or persistence rows.

Money is stored as checked, non-negative minor units plus an assigned uppercase ISO 4217 code.
Multiplication by a three-decimal positive quantity rounds half-up once to the nearest minor unit.
Business settings default to EUR, 25 records per collection, and a hard maximum of 200 records.
Attachment settings enforce one file up to 25 MiB plus a bounded multipart envelope. Startup
rejects invalid settings before serving requests.

## Domain modules and workflow dependencies

Each business area follows the same inward dependency direction:

```text
HTTP controller -> service -> repository contract <- SurrealDB adapter
                         -> domain/model invariants
HTTP DTOs       <- controller mapping <- domain/model values
```

Customer and vehicle services own archive and current-owner workflows. Intervention services own
draft transitions, mileage chronology, line mutations, totals, and deterministic service history.
Technical-note services own reusable-knowledge validation. The shared attachment service owns
vehicle, intervention, and technical-note ownership checks plus the `pending` → `stored` and
`stored` → `deleting` workflows. It coordinates `AttachmentRepository` records with an
`AttachmentFileStore` without pretending those two side effects are one transaction. Invoice
services own draft lines, atomic totals, issued
snapshots and numbering, and append-only payments. Cross-feature checks call repository contracts;
controllers never join records or encode workflow policy.

## Transaction boundary

A service method is the application workflow boundary. When a command must validate related rows
and mutate state atomically, its repository adapter executes one SurrealQL transaction. This
includes intervention-line totals, terminal intervention transitions, invoice-line totals,
issuance and number allocation, and payment balance checks. Controllers perform parsing and DTO
mapping outside that transaction. A workflow never holds a transaction open across an HTTP
response, template render, or another external system.

SurrealDB attachment records and bucket objects are a deliberate non-atomic boundary. Upload first
reserves a `pending` record with an opaque pointer, writes bytes without overwrite, verifies the
object, then exposes it by marking the row `stored`. Delete first marks the row `deleting`, removes
or confirms absence of the object, then removes the row. Ordinary reads expose only `stored` rows.
The explicit dry-run-first reconciliation task reports and safely resumes interrupted states;
startup, readiness, and ordinary requests never perform storage repair.

The one `pipauto_attachments` bucket uses a memory backend only in isolated tests and a mounted
disk backend in Compose. It has `PERMISSIONS NONE`; all content flows through authenticated
application routes. Attachment records, checksums, and opaque file pointers are private persistence
data. A logical database export does not contain the separately mounted bucket bytes, so recovery
always pairs both artifacts. The complete contract is documented in the
[attachment storage guide](attachment-storage.md).

## Schema and SurrealKit ownership

Committed desired definitions under `database/schema/` are the schema source of truth. SurrealKit
owns schema diffing, catalog snapshots, rollout manifests and state, linting, synchronization of
disposable databases, and phased rollout execution for preserved databases. The application owns
runtime queries through repository adapters but does not execute schema changes during boot,
health checks, or ordinary requests. `scripts/surrealkit` owns the secret-safe environment mapping,
authentication-baseline gate, deployment gate, sanitized reports, and rollout lock. Operational
ownership and recovery are defined in [the migration runbook](migrations.md).

## Assets and tests

- `assets/views/layouts` contains reusable complete-page layouts.
- `assets/views/pages` contains full server-rendered pages.
- `assets/views/fragments` contains partial HTML returned to HTMX requests.
- `assets/static/css`, `assets/static/js`, and `assets/static/vendor` contain self-hosted browser assets.
- `tests/requests` verifies public HTTP behavior.
- `tests/integration` verifies infrastructure behavior such as database connectivity.
- `tests/support` contains reusable test bootstrapping, settings, and fixtures.
- `tests/browser` contains Playwright/Axe smoke coverage against a disposable isolated database;
  screenshots, traces, and video are disabled so authentication values are not retained as
  artifacts.
