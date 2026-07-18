# Pipauto

Pipauto is a workshop-oriented Loco application for managing customers, vehicles, and accurate
vehicle service histories.

## Local development

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

Start Pipauto in development mode:

```bash
cargo loco start
```

The application is available at <http://localhost:5150>. The development server stays in the
foreground and can be stopped with `Ctrl+C`.

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

**Destructive:** the following command stops the Compose project and deliberately deletes the
development volume and all local SurrealDB data it contains.

```bash
docker-compose down --volumes
```

The application uses namespace `pipauto` and database `pipauto_development`. Database
`pipauto_test` is reserved for tests that explicitly connect to this standalone server; ordinary
tests should not use the persistent development database.
