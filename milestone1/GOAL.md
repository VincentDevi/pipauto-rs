# Pipauto — Project Setup Milestone

This document is the source of truth for the Linear issues required to complete the first Pipauto milestone, **Project Setup**.

## Milestone outcome

At the end of this milestone, Pipauto has a documented Loco.rs application foundation that:

- Uses server-rendered HTML and a small, self-hosted HTMX enhancement.
- Connects to a standalone SurrealDB instance in development.
- Uses an in-memory SurrealDB engine in automated tests.
- Has explicit module boundaries for HTTP, application logic, domain models, persistence, and presentation.
- Can be set up, run, tested, and diagnosed from the documented commands.

Authentication and all workshop business features are outside this milestone.

## Linear metadata

Apply the following metadata to every issue created from this document:

| Field | Value |
| --- | --- |
| Team | `VincentDevi-Perso` |
| Project | `Pipauto` |
| Milestone | `Project Setup` |
| Assignee | Unassigned |
| Cycle | None |
| Due date | None |

Create the issues in the order below and preserve the dependency relationships stated in each issue.

---

## Issue 1 — Bootstrap the Loco application and document its architecture

**Priority:** High  
**Dependencies:** None

### Objective

Create a minimal Loco.rs application without SeaORM, generated authentication, or a separate frontend application. Establish and document the code structure that later milestones must follow.

### Dependency-management rule

The Loco generator may create the initial manifest. After generation, nobody may edit `Cargo.toml` manually. Add, change, or remove dependencies only through `cargo add`, `cargo remove`, or an official framework generator. Commit both `Cargo.toml` and `Cargo.lock`.

Before generating the project, confirm that the stable Rust compiler is version 1.89 or newer:

```bash
rustc --version
```

Install the Loco command-line tool and generate its **Lightweight Service** starter. The generated application must be located at the repository root; do not retain an extra nested application directory.

```bash
cargo install loco --locked
loco new
```

Add every required direct dependency through Cargo commands:

```bash
cargo add loco-rs
cargo add axum
cargo add async-trait
cargo add serde --features derive
cargo add serde_json
cargo add tera
cargo add thiserror
cargo add tokio --features macros,rt-multi-thread
cargo add tracing
cargo add surrealdb --no-default-features --features protocol-ws,kv-mem,rustls

cargo add --dev insta
cargo add --dev serial_test
```

If the selected Loco starter already added one of these dependencies, run the corresponding `cargo add` command anyway so the required direct dependency and features are declared by Cargo rather than by hand.

### Required application structure

```text
src/
├── app.rs
├── errors.rs
├── controllers/
│   └── mod.rs
├── models/
│   └── mod.rs
├── services/
│   └── mod.rs
├── repositories/
│   ├── mod.rs
│   └── surreal/
│       └── mod.rs
├── database/
│   ├── mod.rs
│   ├── client.rs
│   └── settings.rs
├── initializers/
│   ├── mod.rs
│   ├── surrealdb.rs
│   └── view_engine.rs
└── views/
    └── mod.rs

assets/
├── static/
│   ├── css/
│   ├── js/
│   └── vendor/
└── views/
    ├── layouts/
    ├── pages/
    └── fragments/

tests/
├── requests/
├── integration/
└── support/
```

Every `mod.rs` must start with module-level Rust documentation (`//!`) that explains the module's responsibility, allowed dependencies, and prohibited responsibilities.

### Module responsibilities

| Module or directory | Responsibility |
| --- | --- |
| `app` | Compose application routes, initializers, middleware, and shared services. |
| `controllers` | Parse HTTP input and select responses. Controllers contain no business rules or database queries. |
| `models` | Define database-independent domain values and invariants. It cannot depend on Loco, Axum, Tera, or SurrealDB. |
| `services` | Implement application workflows by coordinating models and repository contracts. |
| `repositories` | Define persistence contracts and contain their adapters under technology-specific submodules. |
| `repositories/surreal` | Implement repository contracts using SurrealDB. It must not contain HTTP or template behavior. |
| `database` | Parse database settings, create and authenticate the SurrealDB client, select its namespace/database, and perform health checks. |
| `initializers` | Wire infrastructure into the Loco lifecycle and shared store. |
| `views` | Build typed presentation data and invoke Tera templates. |
| `errors` | Define application error categories and their HTTP mappings without exposing secrets. |
| `assets/views/layouts` | Hold reusable complete-page layouts. |
| `assets/views/pages` | Hold full server-rendered page templates. |
| `assets/views/fragments` | Hold partial HTML returned to HTMX requests. |
| `tests/requests` | Verify public HTTP behavior. |
| `tests/integration` | Verify infrastructure behavior such as database connectivity. |
| `tests/support` | Provide reusable test bootstrapping, settings, and fixtures. |

Add `docs/architecture.md` documenting this dependency direction:

```text
controllers → services → repository contracts
                     ↘ models
SurrealDB adapters → repository contracts
views ← controllers
app/initializers → compose all infrastructure
```

Business-domain repository traits are not part of this milestone. Create only the foundation needed to add them later.

### Acceptance criteria

- [ ] The stable compiler is Rust 1.89 or newer.
- [ ] The project was generated from the Loco Lightweight Service starter.
- [ ] No dependency was added by manually editing `Cargo.toml`.
- [ ] `cargo metadata` lists every required direct dependency and feature.
- [ ] `Cargo.toml` and `Cargo.lock` are committed.
- [ ] The required directories and documented modules exist.
- [ ] Every `mod.rs` documents its responsibility and dependency restrictions.
- [ ] No generated SeaORM model, migration, authentication, or SPA code remains.
- [ ] `docs/architecture.md` documents the module boundaries and dependency direction.
- [ ] The application compiles and the Loco route command can boot the application definition.

### Verification

```bash
cargo metadata --no-deps
cargo check
cargo test --no-run
cargo loco routes
```

---

## Issue 2 — Provide the local SurrealDB development environment

**Priority:** High  
**Dependencies:** Issue 1

### Objective

Provide a reproducible, persistent SurrealDB service for local development without requiring developers to install the SurrealDB binary directly.

### Implementation requirements

- Add a Compose file with a SurrealDB image pinned to a version supported by the resolved Rust SDK.
- Expose the database on host port `8000`.
- Store database data in a named development volume so ordinary container restarts preserve it.
- Configure a container health check and make the documented application startup wait for it.
- Use local development root credentials supplied through environment variables.
- Use namespace `pipauto`.
- Use database `pipauto_development` for the application.
- Reserve database `pipauto_test` for tests that explicitly target the standalone server.
- Add `.env.example` with safe example values and the full list of required variables.
- Ignore the real `.env` file and any local database data.
- Read all application settings through environment-backed Loco configuration. Rust modules must not contain credentials or environment-specific endpoints.

The README must include exact commands to:

1. Copy the example environment file.
2. Start the database.
3. Check its health.
4. Follow its logs.
5. Stop it without deleting data.
6. Reset it by deliberately deleting the development volume.

The reset operation must be labelled **destructive** immediately before the command.

### Acceptance criteria

- [ ] A new developer can start SurrealDB with one documented Compose command.
- [ ] Port `8000` is available to the application.
- [ ] The service becomes healthy before application startup.
- [ ] Stopping and restarting the container preserves development data.
- [ ] The documented reset command removes local development data.
- [ ] `.env.example` contains names and safe examples but no real secrets.
- [ ] `.env` and local database data are ignored by version control.
- [ ] No credential is hard-coded in Rust, YAML configuration, tests, or documentation.

### Verification

```bash
docker compose config
docker compose up -d surrealdb
docker compose ps
docker compose logs surrealdb
docker compose stop surrealdb
docker compose start surrealdb
```

---

## Issue 3 — Integrate SurrealDB into the Loco application lifecycle

**Priority:** High  
**Dependencies:** Issues 1 and 2

### Objective

Create one application-managed SurrealDB client, verify it during startup, and make it safely available to request handlers.

### Public foundation types

#### `DatabaseSettings`

A typed, validated settings value containing:

- Endpoint.
- Username.
- Password.
- Namespace.
- Database name.
- Connection timeout.
- Engine selection needed to choose WebSocket or in-memory operation.

Validation must reject empty required values, unsupported endpoints, and a zero timeout before attempting a connection. Error output must name the invalid setting but never print the password.

#### `AppDatabase`

A cheap-to-clone wrapper around the SurrealDB client. It must:

- Connect through WebSocket in development and production.
- Use the in-memory engine in automated tests.
- Authenticate remote connections before selecting the namespace and database.
- Select the configured namespace and database before being shared.
- Expose a health operation used by controllers and tests.
- Hide engine-specific types from controllers and future services.

### Loco lifecycle integration

- Construct `AppDatabase` once while the application context is being created.
- Insert it into `AppContext.shared_store`.
- Retrieve it in handlers using Loco's shared-store extractor.
- Fail application startup when configuration, connection, authentication, namespace selection, database selection, or the initial health query fails.
- Do not use a mutable global singleton.
- Do not create a new database connection per request.

### Health endpoint

Add `GET /_health/surrealdb` with a stable JSON representation:

- Return HTTP `200` and a healthy status when the query succeeds.
- Return HTTP `503` and an unavailable status when the query fails after startup.
- Never include endpoints, usernames, passwords, raw query errors, or other connection details in the response.

### Tests

- Unit-test `DatabaseSettings` validation.
- Integration-test successful in-memory initialization and health checking.
- Test invalid and incomplete configuration.
- Request-test both health response shapes by substituting a controllable test database service.

### Acceptance criteria

- [ ] The application connects to the local standalone SurrealDB service.
- [ ] Remote connections authenticate and select the configured namespace/database.
- [ ] Automated tests can use an isolated in-memory engine.
- [ ] Only one shared client is constructed during application boot.
- [ ] Controllers retrieve `AppDatabase` through the shared store.
- [ ] Invalid configuration or credentials prevent normal startup with a useful, secret-free error.
- [ ] `/_health/surrealdb` returns stable `200` and `503` JSON responses.
- [ ] No request path creates a new database connection.

### Verification

```bash
cargo check
cargo test database
curl --fail --silent http://localhost:5150/_health/surrealdb
```

---

## Issue 4 — Implement the initial server-rendered application shell

**Priority:** Medium  
**Dependencies:** Issue 1

### Objective

Render a useful initial Pipauto page from the server and establish the layout, static-asset, controller, and view conventions for later user-interface work.

### Implementation requirements

- Configure Loco's Tera view-engine initializer.
- Configure static-file middleware to serve `assets/static` at `/static`.
- Add `GET /` and render a complete HTML document.
- Create a base layout with:
  - A meaningful document title.
  - UTF-8 and viewport metadata.
  - A header and primary navigation placeholder.
  - A semantic `main` content region.
  - A reusable template content block.
- Create a “Pipauto setup” page explaining that the application foundation is running.
- Pass a typed view model from the controller to the view layer.
- Add a minimal responsive stylesheet that remains readable and operable on phone, tablet, and desktop widths.
- Keep the page fully useful when JavaScript is disabled.

Do not add authentication controls, customer or vehicle navigation, invoice functionality, or other speculative product UI.

### Acceptance criteria

- [ ] `GET /` returns HTTP `200` and a `text/html` content type.
- [ ] The response is a complete semantic HTML document.
- [ ] The title and main setup content are supplied through typed view data.
- [ ] Templates contain no database queries, settings parsing, or application workflows.
- [ ] Static CSS is served from `/static`.
- [ ] The page remains readable and usable at a 320-pixel viewport width.
- [ ] The page provides meaningful content without JavaScript.
- [ ] Request tests assert the title and primary setup content.

### Verification

```bash
cargo test requests
cargo loco start
curl --fail --include http://localhost:5150/
curl --fail --include http://localhost:5150/static/css/app.css
```

---

## Issue 5 — Add a self-hosted HTMX interaction

**Priority:** Medium  
**Dependencies:** Issues 3 and 4

### Objective

Demonstrate the project's progressive-enhancement approach with one small HTMX interaction backed by the shared database service.

### Asset requirements

- Select and pin a stable HTMX 2.x release.
- Download its minified build into `assets/static/vendor/htmx.min.js` with a command; do not paste or recreate the library manually.
- Record the upstream URL, version, and SHA-256 checksum in the README.
- Verify the checksum before accepting an updated asset.
- Load HTMX only from `/static/vendor/htmx.min.js`.
- Do not introduce Node, npm, a bundler, or a runtime CDN dependency.

### Interaction

- Add a setup-status panel to the home page.
- Add a button with `hx-get="/setup/status"` that targets only the status panel.
- Show a visible loading state while the request is running.
- Implement `GET /setup/status` as a thin controller that invokes the shared `AppDatabase` health operation and renders `assets/views/fragments/setup_status.html`.
- Render distinct connected and unavailable states.
- Use textual status and an appropriate live region; color cannot be the only status indicator.
- Return an HTML fragment rather than JSON.
- Add `Vary: HX-Request` wherever representation varies based on the `HX-Request` header.
- Keep the initial page content meaningful if HTMX is unavailable.

### Acceptance criteria

- [ ] HTMX is pinned, checksummed, committed, and served locally.
- [ ] No template references an HTMX CDN.
- [ ] Clicking the status button replaces only the status panel.
- [ ] The fragment endpoint returns `text/html`.
- [ ] Healthy and unavailable database states have different accessible text.
- [ ] The affected response includes `Vary: HX-Request`.
- [ ] The page still explains its status-check action without JavaScript.
- [ ] Request tests cover the HTMX request header, fragment body, content type, `Vary` header, and both health outcomes.

### Verification

```bash
shasum -a 256 assets/static/vendor/htmx.min.js
cargo test requests
curl --fail --include \
  --header 'HX-Request: true' \
  http://localhost:5150/setup/status
curl --fail --include http://localhost:5150/static/vendor/htmx.min.js
```

---

## Issue 6 — Complete milestone verification and developer documentation

**Priority:** Medium  
**Dependencies:** Issues 1–5

### Objective

Make the completed foundation reproducible from a clean checkout and prove that all milestone behaviors work together.

### Documentation requirements

Complete the README with:

- Supported Rust version and required tools.
- Loco installation.
- Environment-file creation.
- SurrealDB start, health, logs, stop, and destructive reset commands.
- Pipauto startup commands.
- Application and database health URLs.
- Route listing.
- Formatting, checking, linting, and test commands.
- The pinned HTMX version, upstream source, and checksum.
- A link to `docs/architecture.md` and this milestone document.
- A troubleshooting section covering:
  - Unsupported Rust compiler versions.
  - Port `8000` or `5150` already being used.
  - Missing container tooling.
  - Unhealthy or stopped SurrealDB containers.
  - Incorrect credentials or namespace/database settings.
  - Missing templates or static assets.
  - Database connection and timeout failures.

Commands in the README must be directly executable and must match the committed paths and configuration.

### Final test coverage

Ensure automated coverage exists for:

- `GET /` full-page rendering.
- `GET /setup/status` as an HTMX fragment.
- `GET /_health/surrealdb` for healthy and unavailable outcomes.
- `GET /static/vendor/htmx.min.js`.
- Database settings validation.
- In-memory database startup and health checking.

### Milestone acceptance checklist

- [ ] A clean checkout can be configured and started using only the README.
- [ ] The local SurrealDB service persists data across normal restarts.
- [ ] The application fails fast for invalid database configuration.
- [ ] The initial page, HTMX fragment, and SurrealDB connection work together.
- [ ] The machine-readable health endpoint never exposes secrets.
- [ ] All documented commands and paths match the repository.
- [ ] Formatting, compilation, linting, tests, and route inspection succeed.
- [ ] No authentication, customer, vehicle, intervention, technical-note, finance, invoice, or calendar implementation is included.

### Verification

Run the complete milestone gate from a clean checkout:

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo loco routes
```

Then follow the README from its first setup step and manually verify:

1. The application starts against the containerized database.
2. `/` renders the initial application shell.
3. The HTMX control updates only the setup-status panel.
4. `/_health/surrealdb` reports a healthy database.

## Foundation interfaces and rules

The milestone exposes only these foundation interfaces:

- `DatabaseSettings`: validated database configuration.
- `AppDatabase`: shared, engine-independent SurrealDB access and health checking.
- `GET /`: full server-rendered HTML shell.
- `GET /setup/status`: HTMX HTML fragment.
- `GET /_health/surrealdb`: machine-readable database health response.

All implementation work must preserve these rules:

1. Controllers remain thin and contain neither business rules nor database queries.
2. Models remain independent of Loco, Axum, Tera, and SurrealDB.
3. Services depend on repository contracts, not SurrealDB implementations.
4. Templates contain presentation behavior only.
5. `app` and `initializers` compose infrastructure; lower-level modules do not locate their own dependencies.
6. SurrealQL uses bound parameters for dynamic values and never interpolates user-controlled values into query strings.
7. Business-domain repositories and features are deferred to their corresponding milestones.

## Technical assumptions

- Local development uses a standalone SurrealDB container.
- Automated tests use the SurrealDB in-memory engine unless a test explicitly verifies the standalone service.
- HTMX is pinned and self-hosted without an npm toolchain.
- Loco's Lightweight Service starter is used because Pipauto does not use Loco's SeaORM model stack.
- Cargo resolves and locks compatible dependency versions during implementation; dependency declarations are never edited manually.
- Implementation follows the current official [Loco project conventions](https://loco.rs/docs/the-app/your-project/), [Loco shared-service pattern](https://loco.rs/docs/extras/pluggability/), and [SurrealDB Rust SDK guidance](https://surrealdb.com/docs/languages/rust).

