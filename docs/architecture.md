# Pipauto architecture

Pipauto is a single Loco application serving HTTP directly. This foundation deliberately contains
no generated authentication, SeaORM models or migrations, separate frontend, or business-domain
repository traits. Those capabilities must be introduced only by later approved milestones.

## Dependency direction

```text
controllers → services → repository contracts
                     ↘ models
SurrealDB adapters → repository contracts
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
| `models` | Database-independent domain values and invariants | Loco, Axum, Tera, or SurrealDB concerns |
| `services` | Application workflows across models and repository contracts | HTTP, templates, or concrete databases |
| `repositories` | Persistence contracts and adapter organization | HTTP, templates, or workflow policy |
| `repositories::surreal` | SurrealDB implementations of repository contracts | HTTP or template behavior |
| `database` | Settings, connection, authentication, database selection, and health checks | Domain persistence contracts or workflows |
| `initializers` | Loco lifecycle wiring and shared-store registration | Business workflows or HTTP behavior |
| `views` | Typed presentation data and Tera rendering | HTTP parsing, business rules, or persistence |
| `errors` | Error categories and secret-safe HTTP mappings | Raw infrastructure details in client responses |

Business-domain repository contracts will live in `repositories` when a later milestone defines the
domain workflows that need them. Their SurrealDB implementations will live in
`repositories::surreal`; connection mechanics remain in `database`.

## Assets and tests

- `assets/views/layouts` contains reusable complete-page layouts.
- `assets/views/pages` contains full server-rendered pages.
- `assets/views/fragments` contains partial HTML returned to HTMX requests.
- `assets/static/css`, `assets/static/js`, and `assets/static/vendor` contain self-hosted browser assets.
- `tests/requests` verifies public HTTP behavior.
- `tests/integration` verifies infrastructure behavior such as database connectivity.
- `tests/support` contains reusable test bootstrapping, settings, and fixtures.
