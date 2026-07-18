# Pipauto

Pipauto is a workshop-oriented Loco application for managing customers, vehicles, and accurate
vehicle service histories.

This repository currently contains only the Project Setup foundation. Authentication and workshop
business features are outside this milestone. See the [architecture](docs/architecture.md) and the
[Project Setup milestone](milestone1/GOAL.md) for its boundaries and decisions.

## Requirements

- Rust 1.89 or newer, including Cargo and rustfmt. Install the stable toolchain with
  [rustup](https://rustup.rs/).
- Docker Engine (or Docker Desktop) with the `docker-compose` command.
- `curl` and `shasum` to refresh and verify the vendored HTMX file.

Confirm the installed versions:

```bash
rustc --version
cargo --version
docker-compose version
curl --version
```

Install the Loco command-line tool:

```bash
cargo install loco --locked
```

Confirm that Cargo can run the project CLI:

```bash
cargo loco --version
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
```

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

Automated tests use an isolated in-memory SurrealDB engine and do not require Docker or `.env`.
Run the complete milestone gate from the repository root:

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo loco routes
```

To apply Rust formatting instead of checking it:

```bash
cargo fmt
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
