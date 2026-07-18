# Pipauto

Pipauto is a workshop-oriented Loco application for managing customers, vehicles, and accurate
vehicle service histories.

## Local development

Docker Compose runs the pinned SurrealDB server, so the SurrealDB binary does not need to be
installed on the host. Its RocksDB files live in the named
`pipauto_surrealdb_development` volume and survive ordinary container stops and restarts.

Copy the example environment file, then replace the example local password in `.env`:

```bash
cp .env.example .env
```

Start SurrealDB and wait until its health check passes:

```bash
docker compose up -d --wait surrealdb
```

Check SurrealDB's health:

```bash
docker compose exec surrealdb /surreal isready --endpoint http://localhost:8000
```

Follow its logs:

```bash
docker compose logs -f surrealdb
```

Start the application after loading `.env`. The Compose `--wait` option prevents application
startup until SurrealDB is healthy:

```bash
set -a && source .env && set +a && docker compose up -d --wait surrealdb && cargo run -- start
```

Stop SurrealDB without deleting its data:

```bash
docker compose stop surrealdb
```

Restart the stopped database with its existing data:

```bash
docker compose start surrealdb
```

**Destructive:** the following command stops the Compose project and deliberately deletes the
development volume and all local SurrealDB data it contains.

```bash
docker compose down --volumes
```

The application uses namespace `pipauto` and database `pipauto_development`. Database
`pipauto_test` is reserved for tests that explicitly connect to this standalone server; ordinary
tests should not use the persistent development database.
