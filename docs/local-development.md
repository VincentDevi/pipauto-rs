# Local development quick start

This guide starts Pipauto against a personal, developer-owned SurrealDB database, brings its
schema up to date, and provisions an application user. Run all commands from the repository root.

> `./scripts/surrealkit sync` is only for disposable or developer-owned databases. Do not use it
> for shared development, staging, or production. Follow the
> [migration and recovery runbook](migrations.md) for those environments.

## First-time setup

Install the tools listed in the [README requirements](../README.md#requirements), then create the
local environment file:

```bash
cp .env.example .env
```

Generate two independent secrets and place one in `PIPAUTO_JWT_SECRET` and the other in
`PIPAUTO_CSRF_SECRET` in `.env`:

```bash
openssl rand -base64 32
openssl rand -base64 32
```

Load the environment, start SurrealDB, apply the complete committed schema, and confirm that no
schema changes remain:

```bash
set -a && source .env && set +a
docker-compose up -d --wait surrealdb
docker-compose exec surrealdb /surreal isready --endpoint http://localhost:8000
./scripts/surrealkit sync
./scripts/surrealkit sync --dry-run
```

The final dry run must report `schema already in sync`. Application startup does not apply schema
changes automatically.

Create the first application user. The task asks for the password twice without echoing it; never
put a password in a command argument or environment variable:

```bash
cargo loco task create_user email:filippo@example.com display_name:Filippo
```

Quote a display name that contains spaces, for example `display_name:'Filippo Rossi'`. Passwords
must contain at least 12 printable Unicode characters and must not equal the normalized email.

Start the application:

```bash
cargo loco start
```

Open <http://localhost:5150> and sign in with the user you created. Stop the foreground server with
`Ctrl+C`.

## Later development sessions

Load the environment, start the existing database, inspect pending schema changes, apply them, and
verify that the schema is current before starting Pipauto:

```bash
set -a && source .env && set +a
docker-compose up -d --wait surrealdb
./scripts/surrealkit sync --dry-run
./scripts/surrealkit sync
./scripts/surrealkit sync --dry-run
cargo loco start
```

Review the first dry run before applying it when the local data matters. The second dry run must
report `schema already in sync`. The separate database and attachment Compose volumes persist
through normal container stops and restarts. A logical database export does not include attachment
bytes; use the [paired backup procedure](migrations.md#paired-database-and-attachment-backup) before
preserving or recovering local stored files.

To add another user, stop the foreground application or run this in a second terminal with `.env`
loaded:

```bash
cargo loco task create_user email:another@example.com display_name:'Another User'
```

For account deactivation, session revocation, and authentication troubleshooting, see the
[authentication operations guide](authentication.md).
