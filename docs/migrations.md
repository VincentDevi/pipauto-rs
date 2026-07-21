# Database migration and recovery operations

## Architecture decision and schema ownership

Pipauto uses SurrealKit `0.7.0` for schema management and SurrealDB `3.2.1` for the
verified deployment baseline. Schema execution is always a separate operator action. Running
`cargo loco start`, restarting the web server, or calling a health endpoint only connects to and
reads the selected database; none of them runs `sync`, `rollout start`, or `rollout complete`.

The committed files under `database/schema/` are the desired-schema source of truth. SurrealKit
owns catalog inspection, schema snapshots, rollout planning, manifest checksums, phased execution,
and rollout metadata. Pipauto's runtime owns repository queries only. A committed rollout is an
immutable deployment artifact once it has started; use a new forward rollout for later changes.

Run every command from the repository root. Load connection settings from an environment-specific
secret source first. The committed `scripts/surrealkit` wrapper maps these application variables to
SurrealKit without putting credentials in arguments or configuration files:

```bash
export SURREALDB_ENDPOINT='wss://database.example'
export SURREALDB_ROOT_USERNAME='operator-name'
read -rs 'SURREALDB_ROOT_PASSWORD?SurrealDB password: '
export SURREALDB_ROOT_PASSWORD
printf '\n'
export SURREALDB_NAMESPACE='pipauto'
export SURREALDB_DATABASE='pipauto_production'
```

For the SurrealDB CLI commands below, set the equivalent secret-safe variables. Its endpoint must
use `http://` or `https://`, not the SDK's `ws://` or `wss://` form:

```bash
export SURREAL_ENDPOINT='https://database.example'
export SURREAL_USER="$SURREALDB_ROOT_USERNAME"
export SURREAL_PASS="$SURREALDB_ROOT_PASSWORD"
export SURREAL_AUTH_LEVEL='root'
```

Do not enable shell tracing while credentials are loaded. Production exports contain production
data even though they are logical SurrealQL files; restrict their directory and every restored
copy to production-data operators.

## Environment policy

| Environment | Command | Data rule | Successful state |
| --- | --- | --- | --- |
| Unit/integration test | `./scripts/surrealkit test --suite 'authentication*'` | Isolated and disposable | The suite exits zero; its temporary database may be discarded. |
| Local personal development, clean database | `./scripts/surrealkit sync` | Disposable or developer-owned | Sync exits zero and a following dry run proposes no changes. |
| Local personal development, existing database | `./scripts/surrealkit sync` | Developer-owned only | Sync exits zero and a following dry run reports `schema already in sync`; review a dry run first when the data matters. |
| Shared development | `./scripts/surrealkit rollout ...` | Preserved | The reviewed rollout reaches its intended terminal state. |
| Staging | `./scripts/surrealkit rollout ...` | Preserved | The rollout and compatible application pass the phased workflow below. |
| Production | backup, then `./scripts/surrealkit rollout ...` | Preserved | A verified export exists before `start`, then the phased workflow completes. |

`sync` is never a shared, staging, or production workflow. Never use
`--allow-shared-prune` as an ordinary command. If recovery ever appears to require it, stop and
write a separate, reviewed procedure based on the specific incident and a verified backup.

## Install the pinned tool

Install and verify the exact version:

```bash
cargo install surrealkit --version 0.7.0 --locked
surrealkit --version
```

The expected version line is `surrealkit 0.7.0`. The wrapper refuses any other version before it
contacts the database.

## Disposable and developer-owned databases

Initialize a clean disposable database after selecting it in the environment:

```bash
./scripts/surrealkit sync
./scripts/surrealkit sync --dry-run
./scripts/surrealkit rollout status
```

The sync must exit zero and the dry run must print `schema already in sync`. On a fresh database,
rollout status may say `No rollout records found.`

Before synchronizing a developer-owned database with data worth keeping, inspect without mutation:

```bash
./scripts/surrealkit sync --dry-run
```

Review every proposed change, then explicitly apply and recheck it:

```bash
./scripts/surrealkit sync
./scripts/surrealkit sync --dry-run
```

Do not use this procedure if anyone else relies on the database. An existing pre-SurrealKit
authentication database must first pass the catalog-gated procedure in
[Authentication operations](authentication.md#setup-and-routine-tasks).

## Baseline adoption for an authentication-only database

Adoption is read-first and must not use `sync`. Run the catalog-gated baseline, then inspect its
status before taking a backup or planning a rollout:

```bash
./scripts/surrealkit baseline-authentication
./scripts/surrealkit rollout status
```

The wrapper compares `INFO FOR DB` and every authentication table with the committed fixture,
fingerprints all authentication records before and after `rollout baseline`, and prints no rows or
sensitive values. Any extra, missing, or changed definition blocks adoption before metadata is
written. Successful adoption adds only SurrealKit metadata and leaves user, session, and throttle
projections unchanged. Export and checksum the database before starting the core rollout.

## Read-only inspection

These are the ordinary preflight and diagnostic commands for any managed database:

```bash
./scripts/surrealkit sync --dry-run
./scripts/surrealkit rollout status
./scripts/surrealkit rollout status "$ROLLOUT_ID"
```

The sync dry run reports schema operations without applying them. `rollout status` lists rollout
records and step states; the targeted form removes ambiguity during a deployment. These commands
initialize SurrealKit metadata on a completely unmanaged database, so first-time adoption must use
the approved baseline procedure. After adoption they do not apply application schema changes.

## Plan and review a rollout

Start from the intended application commit and edit the committed files under `database/schema/`.
Choose a short human-readable name describing one schema outcome. Preview first:

```bash
export ROLLOUT_NAME='add vehicle registration index'
./scripts/surrealkit rollout plan --dry-run --name "$ROLLOUT_NAME"
./scripts/surrealkit rollout plan --name "$ROLLOUT_NAME"
```

The successful plan prints `Generated rollout manifest database/rollouts/<rollout-id>.toml` and
updates the committed schema/catalog snapshots. Set `ROLLOUT_ID` from that filename without the
`.toml` suffix:

```bash
export ROLLOUT_ID='YYYYMMDDHHMMSS__add_vehicle_registration_index'
./scripts/surrealkit rollout lint "$ROLLOUT_ID"
git diff -- database/schema database/rollouts database/snapshots
```

Lint must exit zero and print `Rollout <rollout-id> is valid (checksum <sha256>).` Review the
manifest and snapshots together. Confirm that:

- `id`, `name`, and source/target hashes describe this one change;
- `start` contains only additive, backward-compatible definitions;
- data-changing steps are bounded and safe to retry;
- assertions validate important invariants, including preserved service-history chronology when
  those tables exist;
- `complete` removes or tightens definitions only after old code no longer needs them;
- `rollback` reverses the additive phase without deleting pre-rollout data.

Automatic planning deliberately refuses modified managed entities and ambiguous add/remove pairs.
For those changes, author and review an explicit phased manifest rather than bypassing validation.
Commit the schema files, rollout manifest, and both snapshots as one application change. Run lint
again from the exact commit that will be deployed.

## Deployment gate and phased rollout

The release operator must target one required `ROLLOUT_ID` and capture the status output in the
deployment record. Only `ready_to_complete` permits deployment of application code that depends on
that rollout.

| Status | Deployment decision and action |
| --- | --- |
| `planned` or no record | Block. Run the reviewed `start` phase first. |
| `running_start` | Block. The command was interrupted; inspect steps and use repair. |
| `ready_to_complete` | Permit deployment of compatible code, then smoke-test before `complete`. |
| `running_complete` | Block. Do not deploy or retry blindly; inspect steps and use repair. |
| `running_rollback` | Block. Do not deploy; inspect steps and use repair. |
| `failed` | Block. Read `last_error` and failed steps, fix the cause, then make a reviewed retry-or-rollback decision. |
| unknown state | Block. Preserve evidence and escalate; do not guess a transition. |
| `completed` | Terminal. Rollout rollback is unavailable; use a new forward rollout or disaster recovery. |
| `rolled_back` | Terminal and intentionally abandoned. Do not deploy code that requires this rollout. |

An actionable blocked deployment message must name the environment, rollout ID, observed status,
and next command, for example:

```text
DEPLOYMENT BLOCKED: production rollout 20260719090000__example is running_start.
Inspect: ./scripts/surrealkit rollout status 20260719090000__example
Recovery: follow the interrupted-rollout repair procedure; do not start the application release.
```

Automated deployment validation applies the table above without printing `last_error` or record
contents. It permits only `ready_to_complete`:

```bash
./scripts/surrealkit deployment-gate production "$ROLLOUT_ID"
```

All other states exit non-zero and identify the environment, rollout ID, observed state, and safe
next action. The wrapper also serializes `rollout start` with a database-backed gate lock, so two
concurrent starts cannot both proceed. A successful `repair` or `rollback` clears a lock left by an
interrupted start after the documented state transition is reconciled.

### 1. Export and verify the production backup

Create the backup outside the checkout before `rollout start`. Use a protected absolute directory
owned by the operator account:

```bash
umask 077
export BACKUP_ROOT='/secure/pipauto-backups'
install -d -m 700 "$BACKUP_ROOT"
export BACKUP_CREATED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
export BACKUP_STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
export BACKUP_FILE="${BACKUP_ROOT}/pipauto-${SURREALDB_DATABASE}-${BACKUP_STAMP}.surql"
export BACKUP_METADATA="${BACKUP_FILE}.metadata"
export BACKUP_COUNTS="${BACKUP_FILE}.counts"
export APPLICATION_COMMIT="$(git rev-parse HEAD)"
export SURREALDB_SERVER_VERSION="$(surreal version --endpoint "$SURREAL_ENDPOINT")"

printf '%s\n' \
  'SELECT count() AS count FROM user GROUP ALL;' \
  'SELECT count() AS count FROM auth_session GROUP ALL;' \
  'SELECT count() AS count FROM login_throttle GROUP ALL;' | surreal sql \
    --endpoint "$SURREAL_ENDPOINT" \
    --namespace "$SURREALDB_NAMESPACE" \
    --database "$SURREALDB_DATABASE" \
    --hide-welcome > "$BACKUP_COUNTS"
surreal export \
  --endpoint "$SURREAL_ENDPOINT" \
  --namespace "$SURREALDB_NAMESPACE" \
  --database "$SURREALDB_DATABASE" \
  "$BACKUP_FILE"
test -s "$BACKUP_FILE"
export BACKUP_SHA256="$(shasum -a 256 "$BACKUP_FILE" | awk '{print $1}')"
printf '%s  %s\n' "$BACKUP_SHA256" "$(basename "$BACKUP_FILE")" \
  > "${BACKUP_FILE}.sha256"
(
  printf 'server_version=%s\n' "$SURREALDB_SERVER_VERSION"
  printf 'namespace=%s\n' "$SURREALDB_NAMESPACE"
  printf 'database=%s\n' "$SURREALDB_DATABASE"
  printf 'rollout_id=%s\n' "$ROLLOUT_ID"
  printf 'application_commit=%s\n' "$APPLICATION_COMMIT"
  printf 'created_at=%s\n' "$BACKUP_CREATED_AT"
  printf 'sha256=%s\n' "$BACKUP_SHA256"
) > "$BACKUP_METADATA"
(cd "$BACKUP_ROOT" && shasum -a 256 --check "$(basename "${BACKUP_FILE}.sha256")")
```

Success requires: export exits zero, the file is non-empty, checksum verification prints `OK`, and
the credential-free metadata records server version, namespace, database, rollout ID, exact
application commit, UTC creation time, and checksum. Confirm the counts file contains one result
for each expected table. Copy the backup, checksum, counts, and metadata to the approved protected
storage before continuing. A failed or unverified export blocks production `start`.

### 2. Start the additive phase

Re-lint the exact artifact, check there is no conflicting active rollout, then start:

```bash
./scripts/surrealkit rollout lint "$ROLLOUT_ID"
./scripts/surrealkit rollout status
./scripts/surrealkit rollout start "$ROLLOUT_ID"
./scripts/surrealkit rollout status "$ROLLOUT_ID"
```

Success prints `Rollout <rollout-id> is ready to complete.` The targeted status must be
`ready_to_complete`, with every `start` step completed. Any other state blocks the application
deployment.

### 3. Deploy compatible code and smoke-test

Deploy the reviewed application commit only after the gate passes. Starting or restarting the app
does not advance the rollout:

```bash
cargo loco start
curl --fail --silent --show-error "$PIPAUTO_CANONICAL_ORIGIN/_health"
curl --fail --silent --show-error "$PIPAUTO_CANONICAL_ORIGIN/_health/surrealdb"
./scripts/surrealkit rollout status "$ROLLOUT_ID"
```

Then authenticate with a designated non-production test account where policy permits and exercise
the changed workflow. Confirm existing customer, vehicle, intervention/job, and service-history
reads remain accurate and chronological when those features exist. Exercise invoice reads when
invoice schema exists. Record intentionally unavailable checks as `not applicable`; do not invent
fixtures or production data to satisfy them. The status must remain `ready_to_complete` throughout
smoke testing.

### 4. Complete the contract phase

Only after the compatible app and smoke tests succeed:

```bash
./scripts/surrealkit rollout complete "$ROLLOUT_ID"
./scripts/surrealkit rollout status "$ROLLOUT_ID"
./scripts/surrealkit sync --dry-run
```

Success prints `Completed rollout <rollout-id>.`; targeted status is `completed`, all complete
steps are completed, and the dry run prints `schema already in sync`. `completed` is terminal.
SurrealKit cannot roll it back; correct later problems with a new forward rollout or restore the
backup into the disaster-recovery path below.

For rollout `20260721104500__stored_attachment_schema`, the wrapper first prints the count of
`metadata_only` attachment rows. Capture that count in the deployment record before the contract
phase deletes those legacy rows. If the count cannot be read, the wrapper blocks completion before
SurrealKit runs any contract step.

## Roll back between start and complete

Use rollout rollback when the additive phase succeeded but compatible code cannot safely be
deployed or pass smoke tests. First remove/drain the incompatible application release, then run:

```bash
./scripts/surrealkit rollout status "$ROLLOUT_ID"
./scripts/surrealkit rollout rollback "$ROLLOUT_ID"
./scripts/surrealkit rollout status "$ROLLOUT_ID"
```

Success prints `Rolled back rollout <rollout-id>.`, targeted status is `rolled_back`, and rollback
steps are completed. This is a controlled reversal of the manifest's additive phase. It is not a
database restore and it is unavailable after `completed`. Verify the prior application version and
critical reads after rollback.

## Repair an interrupted rollout

Preserve command output and inspect the targeted record before changing it:

```bash
./scripts/surrealkit rollout status "$ROLLOUT_ID"
./scripts/surrealkit rollout lint "$ROLLOUT_ID"
./scripts/surrealkit rollout repair "$ROLLOUT_ID"
./scripts/surrealkit rollout status "$ROLLOUT_ID"
```

`repair` changes metadata without re-running SQL steps:

- `running_complete` becomes terminal `completed`;
- `running_rollback` becomes terminal `rolled_back`;
- `running_start` becomes `failed`, with instructions to re-run the idempotent `start` or run
  `rollback` after reviewing completed steps;
- terminal states are unchanged; other states are rejected.

After a `running_start` repair or an ordinary `failed` state, inspect `last_error` and each step.
Correct the underlying cause, then have the migration reviewer approve exactly one of:

```bash
./scripts/surrealkit rollout start "$ROLLOUT_ID"
./scripts/surrealkit rollout rollback "$ROLLOUT_ID"
```

For a failure during contract work, approve a retry of `complete` only when its steps are known to
be safe to retry. Do not edit the committed manifest after it has started: SurrealKit checks its
checksum. Do not force metadata to a terminal state or use shared prune as routine repair; either
needs an incident-specific reviewed recovery plan.

## Disaster recovery and restore rehearsal

Disaster recovery restores a logical export because live data or a completed rollout cannot be
made safe with manifest rollback. It is distinct from rollout rollback. Never import over the live
production database. Create a unique isolated recovery database with production-equivalent access
controls and no public application traffic:

```bash
(cd "$BACKUP_ROOT" && shasum -a 256 --check "$(basename "${BACKUP_FILE}.sha256")")
export SOURCE_DATABASE="$SURREALDB_DATABASE"
export RECOVERY_DATABASE="pipauto_recovery_${BACKUP_STAMP}"
test "$RECOVERY_DATABASE" != "$SOURCE_DATABASE"
surreal import \
  --endpoint "$SURREAL_ENDPOINT" \
  --namespace "$SURREALDB_NAMESPACE" \
  --database "$RECOVERY_DATABASE" \
  "$BACKUP_FILE"
```

Import must exit zero. Compare key table counts captured from the source at backup time with the
recovery database without printing rows:

```bash
export RECOVERY_COUNTS="${BACKUP_FILE}.recovery-counts"
printf '%s\n' \
  'SELECT count() AS count FROM user GROUP ALL;' \
  'SELECT count() AS count FROM auth_session GROUP ALL;' \
  'SELECT count() AS count FROM login_throttle GROUP ALL;' | surreal sql \
    --endpoint "$SURREAL_ENDPOINT" \
    --namespace "$SURREALDB_NAMESPACE" \
    --database "$RECOVERY_DATABASE" \
    --hide-welcome > "$RECOVERY_COUNTS"
diff -- "$BACKUP_COUNTS" "$RECOVERY_COUNTS"
```

`diff` must exit zero. If the source database is intentionally quiescent and still available, the
same count query may also be run against `SOURCE_DATABASE` as a current-state comparison.

When customer, vehicle, intervention/job, service-history, and invoice tables exist, add their key
counts and representative chronological service-history and invoice queries to the rehearsal. A
restore is not verified by row counts alone. Point a non-public application instance at the
recovery database, create or use a designated fixture user only where safe, authenticate through
the normal login flow, and run the same application smoke tests used during rollout. For a safe
isolated rehearsal, point Pipauto at the restored database and provision the fixture interactively:

```bash
export SURREALDB_DATABASE="$RECOVERY_DATABASE"
cargo loco task create_user email:recovery-fixture@example.com display_name:'Recovery Fixture'
cargo loco start
```

Enter the fixture password only at the non-echoing prompts. Verify login, customer → vehicle →
intervention/line → deterministic service history, technical-note search, attachment metadata,
invoice issue, partial/final payment status, logout, and rejection of the revoked cookie as
described in [Authentication operations](authentication.md) and [JSON API v1](api-v1.md).
Stop the isolated app after the checks and restore `SURREALDB_DATABASE="$SOURCE_DATABASE"` before
running any other operator command.

Record the import exit status, count comparison, application commit, checks performed, and cleanup
owner beside the backup metadata. Keep or remove the isolated recovery database according to the
approved retention policy; never repoint production at it without a separate disaster-recovery
decision and outage plan.
