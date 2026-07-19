# Pipauto architecture

Pipauto is a single Loco application serving HTTP directly. Shared domain, persistence, service,
and API contracts keep later business areas consistent without coupling them to Loco or SurrealDB.

## Dependency direction

```text
controllers → API DTOs
      ↓
services → domain ← repository contracts
                       ↑
              SurrealDB adapters
views ← controllers
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
| `models` | Feature-specific database-independent models, currently authentication | Loco, Axum, Tera, or SurrealDB concerns |
| `api` | Explicit IDs, timestamps, money, quantity, pagination, and error DTOs | SurrealDB rows, repository errors, or business decisions |
| `services` | Application workflows across models and repository contracts | HTTP, templates, or concrete databases |
| `repositories` | Persistence-neutral errors and contracts using typed domain filters and cursors | HTTP query strings, SurrealDB types, templates, or workflow policy |
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

Money is stored as checked, non-negative minor units plus an assigned uppercase ISO 4217 code.
Multiplication by a three-decimal positive quantity rounds half-up once to the nearest minor unit.
Business settings default to EUR, 25 records per collection, and a hard maximum of 200 records.
Startup rejects invalid settings before serving requests.

## Assets and tests

- `assets/views/layouts` contains reusable complete-page layouts.
- `assets/views/pages` contains full server-rendered pages.
- `assets/views/fragments` contains partial HTML returned to HTMX requests.
- `assets/static/css`, `assets/static/js`, and `assets/static/vendor` contain self-hosted browser assets.
- `tests/requests` verifies public HTTP behavior.
- `tests/integration` verifies infrastructure behavior such as database connectivity.
- `tests/support` contains reusable test bootstrapping, settings, and fixtures.
