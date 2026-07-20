# Pipauto

Pipauto is a workshop-oriented Loco application for managing customers, vehicles, and accurate
vehicle service histories.

The initial core backend is implemented: password authentication, customers, vehicles,
interventions and deterministic service history, searchable technical notes, attachment metadata,
invoices, and append-only payments. See the [architecture](docs/architecture.md),
[JSON API v1](docs/api-v1.md), [authentication guide](docs/authentication.md), and
[migration and recovery runbook](docs/migrations.md). For a concise local workflow, see the
[local development quick start](docs/local-development.md). Frontend maintainers should also read
the [frontend guide](docs/frontend.md).

## Requirements

- Rust 1.89 or newer, including Cargo, rustfmt, and Clippy. Install the stable toolchain with
  [rustup](https://rustup.rs/).
- Docker Engine (or Docker Desktop) with `docker compose` or `docker-compose`.
- Loco CLI compatible with the pinned `loco-rs 0.16.4` application dependency.
- SurrealKit `0.7.0`; the wrapper rejects every other version.
- SurrealDB `3.2.1`, supplied by the pinned Compose image.
- Node.js 18 or newer and npm. CI uses Node.js 22.
- `curl` and `shasum` to refresh and verify the vendored HTMX file.

Confirm the installed versions:

```bash
rustc --version
cargo --version
rustfmt --version
cargo clippy --version
docker-compose version
curl --version
node --version
npm --version
```

Install the Loco and pinned SurrealKit command-line tools:

```bash
cargo install loco --locked
cargo install surrealkit --version 0.7.0 --locked
```

Confirm that Cargo can run the project CLI:

```bash
cargo loco --version
surrealkit --version
```

Install the exact frontend test dependency graph recorded in `package-lock.json`, then install the
Chromium revision pinned by Playwright together with its system dependencies:

```bash
npm ci
npx playwright install --with-deps chromium
```

## First-time setup

Docker Compose runs the pinned SurrealDB server, so the SurrealDB binary does not need to be
installed on the host. Its RocksDB files live in the named
`pipauto_surrealdb_development` volume and survive ordinary container stops and restarts.

Copy the local development environment file. The Docker-only SurrealDB credentials are
`root`/`root`:

```bash
cp .env.example .env
```

Generate two independent secrets and paste them into `.env`:

```bash
openssl rand -base64 32
openssl rand -base64 32
```

Load the environment variables into the current terminal:

```bash
set -a && source .env && set +a
```

Start SurrealDB and wait until its health check passes:

```bash
docker-compose up -d --wait surrealdb
```

Check the container state and database health:

```bash
docker-compose ps
docker-compose exec surrealdb /surreal isready --endpoint http://localhost:8000
```

Apply the complete committed desired schema through the secret-safe wrapper, then create the first
user. Passwords are requested twice through non-echoing terminal
prompts and never belong in the command or environment:

```bash
./scripts/surrealkit sync
./scripts/surrealkit sync --dry-run
cargo loco task create_user email:filippo@example.com display_name:Filippo
```

Start Pipauto in development mode:

```bash
cargo loco start
```

The application is available at <http://localhost:5150>. The development server stays in the
foreground and can be stopped with `Ctrl+C`.

Health URLs:

- Pipauto's machine-readable SurrealDB health: <http://localhost:5150/_health/surrealdb>
- SurrealDB server health: <http://localhost:8000/health>

List all application routes in a second terminal with the environment still loaded:

```bash
cargo loco routes
cargo loco task
```

## Authentication boundary

Pipauto uses administrator-provisioned email/password accounts. A successful login creates a
signed JWT cookie and a matching revocable SurrealDB registry row with the same fixed 12-hour
expiry. Both must remain valid and the user must remain active on every private request. There is
no public registration, refresh token, remember-me behavior, password recovery, role system, or
browser-side token storage.

The login page, static assets, and non-sensitive health endpoints are public. The workshop shell
and every other application route are protected on the server. Unsafe browser requests require an
origin-, action-, expiry-, and session-bound CSRF token; standard forms and HTMX use the same
checks. Logout revokes the registry row before clearing the cookie, so replaying the old JWT fails.

Development cookies are named `pipauto_session` and `pipauto_login_csrf`. Production cookies use
the `__Host-pipauto_session` and `__Host-pipauto_login_csrf` names and require HTTPS. See the
[authentication operations guide](docs/authentication.md) for exact cookie attributes, account
deactivation, session cleanup and revocation, proxy requirements, end-to-end verification, and
authentication troubleshooting.

### Production authentication checklist

- Generate fresh, independent JWT and CSRF secrets with the two `openssl rand -base64 32`
  commands above; store them in the deployment secret manager rather than the repository.
- Set `PIPAUTO_CANONICAL_ORIGIN` to the exact externally visible HTTPS origin and keep
  `PIPAUTO_SESSION_LIFETIME_SECONDS=43200`.
- Terminate HTTPS at a protected deployment boundary and preserve `Set-Cookie`, `Origin`, and
  security headers through the reverse proxy.
- Do not trust or derive client identity from forwarding headers. This release intentionally uses
  the direct socket address; trusted-proxy support has not been configured.
- Run `./scripts/surrealkit sync` for a new development database before starting the application.
  Server startup does not apply schema changes. Existing or shared databases must use the
  catalog-gated baseline or phased rollout workflow in the migration and recovery runbook.

For subsequent development sessions, the complete startup sequence can also be run as one
command:

```bash
set -a && source .env && set +a && docker-compose up -d --wait surrealdb && cargo loco start
```

### Database utilities

Check SurrealDB's health:

```bash
docker-compose exec surrealdb /surreal isready --endpoint http://localhost:8000
```

Follow its logs:

```bash
docker-compose logs -f surrealdb
```

Stop SurrealDB without deleting its data:

```bash
docker-compose stop surrealdb
```

Restart the stopped database with its existing data:

```bash
docker-compose start surrealdb
```

Wait for a restarted database to become healthy:

```bash
docker-compose up -d --wait surrealdb
```

**Destructive:** the following command stops the Compose project and deliberately deletes the
development volume and all local SurrealDB data it contains.

```bash
docker-compose down --volumes
```

The application uses namespace `pipauto` and database `pipauto_development`. Database
`pipauto_test` is reserved for tests that explicitly connect to this standalone server; ordinary
tests should not use the persistent development database.

## Development checks

Rust tests use an isolated in-memory SurrealDB engine and do not require Docker or `.env`. Run the
complete migration and application gate from the repository root. It starts a disposable
in-memory SurrealDB database named `pipauto_ci`, rejects development and production database names,
and removes the container even when a check fails:

```bash
./scripts/ci-check
```

The gate runs formatting, checking, Clippy with warnings denied, every isolated SurrealKit suite,
lint for every committed rollout manifest, the Rust migration integration tests, the complete Rust
suite, and the Loco route/task inventories. SurrealKit's
machine-readable result is sanitized to `artifacts/migration-report.json`: it includes suite/case
names and pass/fail state, but omits connection settings, database names, error payloads, and rows.
CI uploads that report only when the gate fails.

SurrealKit is pinned at `0.7.0` and verified with the SurrealDB `3.2.1` server image and Rust SDK.
The wrapper maps `SURREALDB_ENDPOINT`, `SURREALDB_ROOT_USERNAME`,
`SURREALDB_ROOT_PASSWORD`, `SURREALDB_NAMESPACE`, and `SURREALDB_DATABASE` to SurrealKit's
connection variables without writing credentials to configuration or command arguments. It rejects
a missing/different CLI version or incomplete settings before contacting the database.

To apply Rust formatting instead of checking it:

```bash
cargo fmt
```

### Frontend browser checks

Run the complete Playwright and Axe suite from the repository root after `npm ci` and the Chromium
installation above:

```bash
npx playwright test
```

The Playwright server command starts a dedicated in-memory SurrealDB container, creates the
`pipauto_browser/browser_smoke` disposable database, applies the committed schema with
`./scripts/surrealkit sync`, verifies the dry run is clean, provisions one synthetic fixture user,
and starts Pipauto. It always removes the container and volume when the run ends. It refuses to
reuse an existing application server and never targets the preserved development database.

The suite runs serially against desktop, tablet, JavaScript-disabled desktop, and phone Chromium
projects. Axe assertions are part of the browser specs; there is no separate accessibility command.
Screenshots, videos, and traces are disabled so session cookies, CSRF values, and entered fixture
credentials are not retained. See the [frontend guide](docs/frontend.md#browser-tests-and-fixtures)
for scenario coverage and safe fixture rules.

To run a focused project or tagged workflow:

```bash
npx playwright test --project=phone-chromium
npx playwright test --project=no-javascript
npx playwright test --grep @invoice-lifecycle
```

## Vendored browser assets

HTMX is served by Pipauto itself; the application has no runtime CDN dependency.

- Version: `2.0.10`
- Upstream URL: `https://cdn.jsdelivr.net/npm/htmx.org@2.0.10/dist/htmx.min.js`
- SHA-256: `71ea67185bfa8c98c39d31717c6fce5d852370fcdfd129db4543774d3145c0de`

Download the pinned minified build with:

```bash
mkdir -p assets/static/vendor
curl --fail --location --silent --show-error \
  --output assets/static/vendor/htmx.min.js \
  https://cdn.jsdelivr.net/npm/htmx.org@2.0.10/dist/htmx.min.js
```

Before accepting an updated asset, replace the expected checksum only after confirming it from a
trusted upstream source, then verify the downloaded bytes:

```bash
printf '%s  %s\n' \
  '71ea67185bfa8c98c39d31717c6fce5d852370fcdfd129db4543774d3145c0de' \
  'assets/static/vendor/htmx.min.js' | shasum -a 256 --check
```

## Troubleshooting

### Unsupported Rust compiler

Pipauto requires Rust 1.89 or newer. If `rustc --version` reports an older compiler, update and
select stable Rust, then retry the failing Cargo command:

```bash
rustup update stable
rustup default stable
rustc --version
```

### Port 8000 or 5150 is already in use

SurrealDB binds port `8000` and Pipauto binds port `5150`. Stop the other process or container using
the port before starting this project. These commands identify common listeners:

```bash
lsof -nP -iTCP:8000 -sTCP:LISTEN
lsof -nP -iTCP:5150 -sTCP:LISTEN
docker-compose ps
```

### Container tooling is missing

If `docker-compose` is not found or cannot connect to the daemon, install and start Docker Engine or
Docker Desktop, confirm `docker-compose version`, and rerun the setup command. The SurrealDB binary
does not need to be installed on the host.

### SurrealDB is stopped or unhealthy

Inspect its state and recent logs, then recreate or restart the service and wait for health:

```bash
docker-compose ps
docker-compose logs --tail 100 surrealdb
docker-compose up -d --wait surrealdb
docker-compose exec surrealdb /surreal isready --endpoint http://localhost:8000
```

If a disposable local database remains unhealthy, use the destructive reset command under
[Database utilities](#database-utilities), then repeat the first-time setup.

### Credentials, namespace, or database settings are incorrect

The values loaded from `.env` configure both Compose and Pipauto. Recreate `.env` if needed, load it
again, and recreate the container so all values agree:

```bash
cp .env.example .env
set -a && source .env && set +a
docker-compose up -d --force-recreate --wait surrealdb
cargo loco start
```

Do not print or commit real credentials. Pipauto validates required settings at startup and reports
the invalid setting name without exposing its value.

### Templates or static assets are missing

Run the request tests to check the committed templates, stylesheet, and vendored HTMX file. From a
clean checkout, the required paths are under `assets/views` and `assets/static`:

```bash
test -f assets/views/pages/setup.html
test -f assets/views/fragments/setup_status.html
test -f assets/static/css/app.css
test -f assets/static/vendor/htmx.min.js
cargo test requests::setup
```

### Database connection or timeout failures

Confirm `.env` is loaded, the endpoint is `ws://localhost:8000`, and SurrealDB is healthy before
starting Pipauto:

```bash
set -a && source .env && set +a
docker-compose up -d --wait surrealdb
docker-compose exec surrealdb /surreal isready --endpoint http://localhost:8000
cargo loco start
```

Startup intentionally fails if connection, authentication, namespace/database selection, or the
initial health query fails or exceeds the configured five-second timeout. Use
`docker-compose logs --tail 100 surrealdb` alongside the Pipauto startup output to distinguish a
stopped service from invalid settings.

### Playwright cannot start Chromium

Reinstall the browser revision and operating-system libraries selected by the locked Playwright
package, then retry:

```bash
npm ci
npx playwright install --with-deps chromium
npx playwright test
```

If installation fails, confirm Node.js is at least version 18 and that the machine can reach the
npm registry and Playwright browser download host. Do not replace `npm ci` with `npm install` in CI.

### Browser tests cannot start the disposable database

The browser suite needs Docker, ports `18000` and `5150`, the pinned `surrealkit` command, and no
already-running Pipauto server. Inspect listeners and the isolated Compose project, then rerun:

```bash
lsof -nP -iTCP:18000 -sTCP:LISTEN
lsof -nP -iTCP:5150 -sTCP:LISTEN
docker-compose --project-name pipauto-browser-smoke --file compose.browser.yaml ps
npx playwright test
```

The harness cleans its disposable volume on normal exit and before every run. Never point it at a
development, staging, or production database to recover a failed test.
