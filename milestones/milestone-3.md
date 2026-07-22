# Pipauto — Database Migrations and Core Backend Milestone

This document is the source of truth for the Linear issues required to complete the third Pipauto
milestone, **Database Migrations and Core Backend**.

It is based on Linear milestone `eb4ac594-0b11-4107-b603-10155dee1516` and the
`VincentDevi/pipauto-rs` `main` branch at commit `7837e4f`. If the implementation branch has moved
forward, compare it with that commit before starting Issue 1 and update this document only when a
newly approved product or architecture decision requires it.

## Milestone outcome

At the end of this milestone, Pipauto has:

- A version-controlled SurrealDB schema managed through SurrealKit.
- A safe adoption path for databases that already contain Pipauto users and authentication
  sessions.
- Fast schema synchronization for disposable local and test databases.
- Reviewed, phased, recoverable schema rollouts for shared, staging, and production databases.
- Strict schemas for customers, vehicles, interventions, line items, technical notes, attachment
  metadata, invoices, invoice lines, and payments.
- Database-independent domain models, repository contracts, and application services following the
  dependency direction already documented in `docs/architecture.md`.
- An authenticated, CSRF-protected, versioned JSON API under `/api/v1`.
- Deterministic vehicle service-history queries and consistent invoice/payment calculations.
- Automated migration, schema, repository, service, request, and end-to-end tests.
- Operational documentation for initialization, rollout, backup, rollback, repair, and recovery.

The milestone supplies backend contracts for the later server-rendered frontend. It does not
implement that frontend.

## Out of scope

- Server-rendered customer, vehicle, intervention, knowledge, or invoice pages.
- Binary file or image storage, upload protocols, buckets, object keys, transformations, or signed
  download URLs.
- Appointments, calendars, reminders, inventory, and parts-stock management.
- Belgian VAT rules, jurisdiction-specific invoice wording, accounting exports, credit notes, or
  fiscal integrations.
- Payment-provider integrations, contactless payments, and online payments.
- Email delivery or customer-facing portals.
- Roles, workshop organizations, granular permissions, or multi-tenancy.
- Ownership-transfer history for vehicles.
- Embeddings, vector indexes, retrieval-augmented generation, agents, MCP endpoints, or an AI
  mechanic assistant.

The schema should retain useful relationships and searchable technical knowledge for later
features, but future AI possibilities do not justify AI-specific infrastructure in this milestone.

## Linear metadata

Apply the following metadata to every issue created from this document:

| Field | Value |
| --- | --- |
| Team | `VincentDevi-Perso` |
| Project | `Pipauto` |
| Milestone | `Database Migrations and Core Backend` |
| Assignee | Unassigned |
| Cycle | None |
| Due date | None |

Create the issues in the order below and preserve the dependency relationships stated in each
issue. Issue numbers in this document are dependency aliases, not final Linear identifiers. After
creation, replace aliases in Linear with actual blocking/blocked-by relationships.

## Investigated migration decision

### Existing state

Pipauto currently uses one application-managed `Surreal<Any>` client, installed as `AppDatabase`
in Loco's shared store. Authentication persistence is implemented through database-independent
repository traits and `SurrealAuthRepository`. The `user`, `auth_session`, and `login_throttle`
schema is held in the Rust constant `database::schema::AUTH_SCHEMA` and applied by the explicit
Loco task `apply_auth_schema`.

That mechanism is intentionally idempotent, but it has no ordered schema history, catalog drift
check, staged deployment, rollout state machine, or supported recovery after partial application.
It must be replaced without recreating the existing tables and without modifying or deleting
authentication records.

### Selected approach

SurrealKit is the migration authority:

- Committed `.surql` files under `database/schema/` describe desired schema state.
- Disposable developer databases and isolated CI databases use `surrealkit sync`.
- Existing databases first mirror and baseline their current complete schema.
- Shared, staging, and production databases use reviewed rollout manifests.
- Rollout `start` performs additive/compatible work, application deployment occurs next, and
  rollout `complete` performs any approved contract/removal phase only after verification.
- Rollout metadata is inspected with `status`; interrupted states use the documented `rollback` or
  `repair` path.
- Normal application startup never mutates production schema.

The implementation must pin the selected SurrealKit version in developer and CI setup, record the
exact version in `docs/migrations.md`, and prove compatibility with the repository's pinned
SurrealDB server and Rust SDK before baselining.

### Rejected alternatives

- **Keep `apply_auth_schema` and add more constants:** rejected because it cannot provide reviewed
  ordered rollouts, catalog snapshots, concurrency protection, or a reliable recovery state.
- **Build a custom Rust migration runner:** rejected because Pipauto would own migration locking,
  history, checksums, drift, interrupted-state recovery, and rollback logic already supplied by the
  official SurrealDB tooling.
- **Apply schema automatically in `App::after_context`:** rejected because an ordinary process
  restart must not silently mutate a shared or production database.
- **Use sync against production:** rejected because desired-state pruning is suitable only for
  disposable databases; production requires an explicit expand/contract rollout.
- **Introduce SeaORM migrations:** rejected because Pipauto deliberately uses SurrealDB directly
  and must not add a second persistence stack.

References:

- [Loco quick tour](https://loco.rs/docs/getting-started/tour/)
- [SurrealKit schema migration](https://surrealdb.com/docs/manage/schema-migration)
- [Adopting an existing database](https://surrealdb.com/docs/manage/schema-migration/getting-started/existing-databases)
- [Sync versus rollouts](https://surrealdb.com/docs/manage/schema-migration/getting-started/sync-vs-rollouts)
- [Rollouts](https://surrealdb.com/docs/manage/schema-migration/rollouts)
- [SurrealKit testing](https://surrealdb.com/docs/manage/schema-migration/testing)
- [SurrealDB backup and recovery](https://surrealdb.com/docs/manage/self-hosted/backups-and-recovery)
- [SurrealDB transactions](https://surrealdb.com/docs/learn/querying/concepts-and-guides/transactions)
- [Record references and deletion behavior](https://surrealdb.com/docs/reference/query-language/language-primitives/record-references)
- [AI agents and context layers](https://surrealdb.com/use-cases/ai-agents)

## Shared schema and API decisions

These decisions apply to all issues and must not be re-decided independently.

### Persistence conventions

- Business tables are `SCHEMAFULL` and explicitly declare every field.
- The application continues to connect with server credentials; browser identity is enforced by
  the existing Loco/Axum authentication boundary.
- Table permissions remain `NONE` for direct unauthenticated database access. This milestone does
  not introduce SurrealDB record authentication.
- Dynamic SurrealQL values are always bound parameters. Record/table names may only come from
  closed application-owned enums, never request strings.
- Domain models and repository traits contain no SurrealDB types. Adapters parse and render record
  IDs at the persistence boundary.
- API IDs are opaque strings. Clients must not parse table names or record keys from them.
- `created_at` is database-generated UTC and read-only. `updated_at` is database-generated UTC and
  refreshed on persisted updates. Optional `archived_at`, `completed_at`, `issued_at`, `voided_at`,
  and `received_at` values are UTC.
- Record references declare `REFERENCE ON DELETE REJECT` unless this document explicitly requires
  owned-child cleanup. Archiving is preferred over deletion.
- Search normalization is deterministic, tested, and shared by writes and queries. VINs are
  trimmed and ASCII-uppercased. Registrations are trimmed, ASCII-uppercased, and stripped of spaces
  and common separators for lookup while preserving the submitted display value.
- User-entered descriptive text remains human-authored content; normalization fields must not
  overwrite display values.

### Money and quantity

- Money uses `{ amount_minor: i64, currency: String }` in public DTOs and corresponding typed
  domain values. Amounts in this milestone must be non-negative.
- Currency is an uppercase three-letter ISO 4217 code. Application configuration defaults to
  `EUR`; every invoice, invoice line, and payment must use the invoice currency.
- Quantities are positive decimal strings in JSON and decimal values in persistence. Accept at
  most three fractional digits and reject exponent notation, zero, negatives, NaN, and infinity.
- Multiplying quantity by unit price rounds once to the nearest minor unit using decimal
  half-away-from-zero. Persist the calculated line total so issued financial snapshots never
  change because of later code changes.
- Financial totals are calculated in services with checked arithmetic. Integer overflow is a
  validation failure, never a wrapped value.

### Lifecycle and deletion

- Customers, vehicles, and technical notes support archive and restore operations.
- Archived customers cannot receive new vehicles. Archived vehicles cannot receive new
  interventions, invoices, or attachment metadata.
- Historical records linked to an archived parent remain readable.
- Interventions are never hard-deleted through the API; an unwanted draft is cancelled.
- Completed interventions retain their recorded chronology and are not generally editable.
- Issued invoices and payments are never hard-deleted. Draft invoices may be voided rather than
  deleted so audit behavior stays uniform.
- Child line items may be removed only while their intervention or invoice is editable.
- Attachment metadata may be deleted only while its storage state is `metadata_only`; later binary
  storage lifecycle rules belong to the image-storage milestone.

### JSON API conventions

- All business routes are below `/api/v1` and return JSON only.
- Every route requires `CurrentUser`. Unsafe methods also require the existing session-bound CSRF
  validation. Authentication and CSRF are enforced on the server, not inferred from the UI.
- Collection responses use `{ "data": [...], "next_cursor": "..." | null }`.
- The default page size is 25 and the maximum is 100. Cursors are opaque, URL-safe, authenticated
  encodings of the final sort tuple; malformed or filter-mismatched cursors return validation
  errors.
- Service history sorts by `service_date DESC, created_at DESC, id DESC`. Other collections use a
  documented stable sort ending in `id`.
- Timestamps use RFC 3339 UTC. Decimal quantities are JSON strings. Money contains integer minor
  units and currency.
- Error responses use:

```json
{
  "error": {
    "code": "validation_failed",
    "message": "Check the submitted values.",
    "fields": { "email": ["Enter a valid email address."] },
    "correlation_id": null
  }
}
```

- Supported codes include `malformed_request`, `validation_failed`, `unauthenticated`,
  `forbidden`, `not_found`, `conflict`, `database_unavailable`, and `internal_error`.
- Expected mappings are HTTP 400 for malformed syntax, 401 for missing/stale authentication, 403
  for CSRF/access rejection, 404 for absent records, 409 for uniqueness/state/relationship
  conflicts, 422 for semantic validation, 503 for unavailable persistence, and 500 for opaque
  internal failures.
- Raw SurrealDB errors, queries, credentials, record internals, and secrets never cross the HTTP
  boundary.

## Core relationship and deletion matrix

| Child field | Parent | Required | Parent deletion | Child lifecycle |
| --- | --- | --- | --- | --- |
| `vehicle.customer` | `customer` | Yes | Reject | Archive/restore |
| `intervention.vehicle` | `vehicle` | Yes | Reject | Draft → completed/cancelled |
| `intervention_line.intervention` | `intervention` | Yes | Reject | Editable only with draft parent |
| `technical_note.vehicle` | `vehicle` | No | Reject while present | Archive/restore |
| `technical_note.source_intervention` | `intervention` | No | Reject while present | Archive/restore |
| `attachment.vehicle` | `vehicle` | Exactly one owner | Reject | Metadata-only delete restriction |
| `attachment.intervention` | `intervention` | Exactly one owner | Reject | Metadata-only delete restriction |
| `invoice.customer` | `customer` | Yes | Reject | Draft → issued/void |
| `invoice.vehicle` | `vehicle` | No | Reject while present | Draft → issued/void |
| `invoice.intervention` | `intervention` | No | Reject while present | Draft → issued/void |
| `invoice_line.invoice` | `invoice` | Yes | Reject | Editable only with draft parent |
| `payment.invoice` | `invoice` | Yes | Reject | Append-only correction policy |

## Planned route inventory

Exact Rust function names are implementation details, but the method/path contracts below are
public and must be documented and tested.

| Area | Routes |
| --- | --- |
| Customers | `GET/POST /api/v1/customers`; `GET/PATCH /api/v1/customers/{id}`; `POST /api/v1/customers/{id}/archive`; `POST /api/v1/customers/{id}/restore`; `GET /api/v1/customers/{id}/vehicles` |
| Vehicles | `GET/POST /api/v1/vehicles`; `GET/PATCH /api/v1/vehicles/{id}`; `POST /api/v1/vehicles/{id}/archive`; `POST /api/v1/vehicles/{id}/restore`; `GET /api/v1/vehicles/{id}/service-history` |
| Interventions | `GET/POST /api/v1/interventions`; `GET/PATCH /api/v1/interventions/{id}`; `POST /api/v1/interventions/{id}/complete`; `POST /api/v1/interventions/{id}/cancel` |
| Intervention lines | `GET/POST /api/v1/interventions/{id}/lines`; `PATCH/DELETE /api/v1/interventions/{id}/lines/{line_id}` |
| Technical notes | `GET/POST /api/v1/technical-notes`; `GET/PATCH /api/v1/technical-notes/{id}`; `POST /api/v1/technical-notes/{id}/archive`; `POST /api/v1/technical-notes/{id}/restore` |
| Attachments | `GET/POST /api/v1/vehicles/{id}/attachments`; `GET/POST /api/v1/interventions/{id}/attachments`; `GET/PATCH/DELETE /api/v1/attachments/{id}` |
| Invoices | `GET/POST /api/v1/invoices`; `GET/PATCH /api/v1/invoices/{id}`; `POST /api/v1/invoices/{id}/issue`; `POST /api/v1/invoices/{id}/void` |
| Invoice lines | `GET/POST /api/v1/invoices/{id}/lines`; `PATCH/DELETE /api/v1/invoices/{id}/lines/{line_id}` |
| Payments | `GET/POST /api/v1/invoices/{id}/payments`; `GET /api/v1/payments/{id}` |

## Dependency graph

```text
Issue 1 ──→ Issue 2 ──→ Issue 3
   │
   └──────→ Issue 4
              ├──→ Issue 5 ──┐
              ├──→ Issue 6 ──┤
              ├──→ Issue 7 ──┼──→ Issue 9
              └──→ Issue 8 ──┘

Issues 3 + 4 ──→ Issue 10
Issues 5 + 9 + 10 ──→ Issue 11
Issues 6 + 9 + 10 + 11 ──→ Issue 12
Issues 7 + 9 + 10 + 11 + 12 ──→ Issue 13
Issues 8 + 9 + 10 + 11 + 12 ──→ Issue 14
Issues 1–14 ──→ Issue 15
```

Issues 5–8 may be implemented in parallel after Issue 4 because each owns separate schema files.
Issue 3 may proceed in parallel with Issues 4–8 after the SurrealKit foundation exists. API domain
issues intentionally follow the shared API foundation and the verified core rollout.

---

## Issue 1 — Adopt SurrealKit and baseline the existing authentication schema

- **Priority:** High
- **Dependencies:** Handle user auth milestone
- **Blocks:** Issues 2–4

### Objective

Replace the one-off authentication schema task with an official, version-controlled SurrealKit
foundation without changing existing authentication definitions or data.

### Tooling and repository layout

- Verify the current SurrealDB server and Rust SDK versions, then select and pin a compatible
  SurrealKit release in the developer and CI setup documentation.
- Initialize and commit `surrealkit.toml` plus:

```text
database/
├── schema/
│   ├── authentication/
│   │   ├── user.surql
│   │   ├── auth_session.surql
│   │   └── login_throttle.surql
│   └── business/
├── rollouts/
├── snapshots/
├── seed/
└── tests/
```

- Do not add SurrealKit as a runtime application dependency merely to invoke the CLI.
- Map the existing `SURREALDB_*` configuration to SurrealKit without committing credentials.

### Authentication-schema mirroring

- Copy every effective table, field, assertion, default, read-only rule, and index from
  `AUTH_SCHEMA` into the authentication `.surql` files.
- Query `INFO FOR DB`, `INFO FOR TABLE user`, `INFO FOR TABLE auth_session`, and
  `INFO FOR TABLE login_throttle` against a representative existing database.
- Compare normalized catalog definitions with the committed files. Stop if the live catalog
  contains an unknown definition or differs materially from the expected schema.
- Capture SurrealKit baseline snapshots only after the full live schema is represented in files.
- Treat baseline as read-only: row counts, record IDs, timestamps, hashes, active states, session
  expiries, and throttle state must remain unchanged.

### Compatibility transition

- Keep `apply_auth_schema` available until clean-database initialization and existing-database
  baseline verification both pass through SurrealKit.
- Switch the README and authentication guide to SurrealKit only in the same change that retires the
  task and Rust schema constant.
- Remove the task registration, constant, obsolete helper, and tests only after replacement tests
  cover their behavior.
- A deployment must never have two competing schema authorities or no schema command.

### Failure behavior

- Missing SurrealKit, an unsupported version, incomplete credentials, catalog drift, or a baseline
  mismatch must fail before any schema mutation.
- Error messages may name a definition and phase but must not print credentials, password hashes,
  raw sessions, JWTs, or complete database exports.

### Acceptance criteria

- [ ] SurrealKit and SurrealDB compatibility is verified and the chosen version is pinned.
- [ ] All existing authentication definitions are represented in committed `.surql` files.
- [ ] Catalog comparison detects missing, extra, and changed definitions.
- [ ] Baseline snapshot generation does not mutate schema or records.
- [ ] Existing users, sessions, and throttle records are byte-for-byte equivalent at the logical
      field level after adoption.
- [ ] Clean databases can receive the authentication schema through SurrealKit.
- [ ] `AUTH_SCHEMA` and `apply_auth_schema` are retired only after the compatibility gate passes.
- [ ] No credential or sensitive authentication value is committed or printed.

### Verification

```bash
surrealkit --version
surrealkit test --suite 'authentication*'
cargo test auth_repositories
cargo loco task
```

---

## Issue 2 — Define the migration lifecycle and production operations

- **Priority:** High
- **Dependencies:** Issue 1
- **Blocks:** Issue 3

### Objective

Define one safe, repeatable migration lifecycle for disposable, shared, and production databases,
including backup, rollout, rollback, repair, and disaster recovery.

### Environment policy

| Environment | Allowed schema command | Data expectation |
| --- | --- | --- |
| Unit/integration test | Isolated sync | Disposable |
| Local personal development | Explicit sync | Disposable or developer-owned |
| Shared development | Rollout | Preserved |
| Staging | Rollout | Preserved |
| Production | Rollout after backup | Preserved |

Never recommend `--allow-shared-prune` as an ordinary command. If it is ever required, it needs a
separate reviewed recovery procedure.

### Required command workflows

Document exact commands and expected successful states for:

1. Installing the pinned SurrealKit version.
2. Initializing a clean disposable database.
3. Synchronizing a developer-owned database.
4. Inspecting schema and rollout status without mutation.
5. Planning and naming a rollout.
6. Reviewing and linting the generated manifest.
7. Exporting a pre-rollout backup.
8. Starting the additive phase.
9. Deploying and smoke-testing compatible application code.
10. Completing the contract phase.
11. Rolling back after `start` but before `complete`.
12. Repairing interrupted rollout metadata.
13. Restoring an export into a separate recovery database and verifying it.

Migration execution remains an explicit deployment action. `cargo loco start`, health checks, and
ordinary web-server restarts must not apply schema changes.

### Rollout gate

- Refuse application deployment when rollout status is `running_start`, `failed`,
  `running_rollback`, or another unapproved intermediate state.
- Permit application deployment only after the required rollout reaches `ready_to_complete`.
- Run application smoke tests before `complete`.
- Treat `completed` and intentionally `rolled_back` as terminal states.
- Document that rollback is unavailable after completion; later recovery uses a new forward
  rollout or restored backup.

### Backup and recovery

- Use `surreal export` to create a timestamped logical backup before production rollout start.
- Write backups outside the repository and restrict access as production data.
- Record the server version, namespace, database, rollout ID, application commit, creation time,
  and checksum beside the backup without embedding credentials.
- Verify a backup by importing it into an isolated namespace/database, checking key table counts,
  authenticating a fixture user where safe, and running representative service-history and invoice
  queries once those schemas exist.
- Never test restore by overwriting the live production database.

### Acceptance criteria

- [ ] Every environment has one documented schema workflow.
- [ ] Startup remains schema-read-only.
- [ ] Rollout plan, lint, start, status, complete, rollback, and repair are documented.
- [ ] Intermediate and failed states block deployment with actionable output.
- [ ] Production rollout requires a successful, checksummed logical export.
- [ ] Restore rehearsal targets an isolated database and includes application-level checks.
- [ ] The difference between rollout rollback and disaster recovery is explicit.
- [ ] Examples use environment variables or prompts without leaking credentials.

### Verification

```bash
surrealkit rollout status
surrealkit rollout lint <rollout-id>
surreal export --namespace pipauto --database pipauto_development /tmp/pipauto-backup.surql
surreal import --namespace pipauto --database pipauto_recovery /tmp/pipauto-backup.surql
```

---

## Issue 3 — Add migration validation and CI safety gates

- **Priority:** High
- **Dependencies:** Issues 1 and 2
- **Blocks:** Issues 10 and 15

### Objective

Make schema drift, unsafe rollouts, partial failures, and authentication-data regressions fail in
automated verification before deployment.

### SurrealKit tests

- Add schema-metadata assertions for every authentication table, field, type, assertion, and
  index.
- Add schema-behavior tests proving normalized email uniqueness, session-digest uniqueness,
  session expiry lookup behavior, and throttle composite uniqueness.
- Run each suite against its own isolated database.
- Provide a machine-readable report artifact when CI fails without including credentials or row
  contents.

### Rust migration integration tests

Build reusable fixtures for:

- A clean database with no Pipauto definitions.
- An existing authentication database with representative active/inactive users, active/revoked/
  expired sessions, and throttle rows.
- A deliberately drifted database with an extra field, missing index, or changed assertion.

Test clean sync, repeated sync, non-mutating baseline inspection, pending rollout application,
catalog comparison, and preservation of authentication fixtures.

### Rollout-state tests

- Lint every committed rollout manifest.
- Exercise `planned`, `running_start`, `ready_to_complete`, `running_complete`, `completed`,
  `running_rollback`, `rolled_back`, and `failed` handling.
- Verify two concurrent rollout starts cannot both proceed.
- Simulate interruption at safe test boundaries and verify documented repair/rollback commands.
- Assert failure output identifies the phase and rollout without exposing connection secrets or
  record data.

### CI gate

Add one documented check that runs formatting/checking, SurrealKit tests, rollout lint, migration
integration tests, and the existing Rust suite. The CI database must be disposable and must never
point to `pipauto_development` or production.

### Acceptance criteria

- [ ] Authentication schema metadata and behavior have declarative tests.
- [ ] Clean and existing-database paths are covered.
- [ ] Drift is detected before mutation.
- [ ] Repeated disposable sync is safe.
- [ ] Every relevant rollout state has explicit gate behavior.
- [ ] Concurrent rollout execution is rejected.
- [ ] Authentication fixture records survive baseline and business rollout testing.
- [ ] CI fails on schema drift, invalid manifests, migration failures, or secret leakage.

### Verification

```bash
surrealkit test
surrealkit rollout lint <each-committed-rollout>
cargo test migration
cargo test
```

---

## Issue 4 — Establish shared domain, persistence, and API conventions

- **Priority:** High
- **Dependencies:** Issue 1
- **Blocks:** Issues 5–8 and 10

### Objective

Create reusable domain primitives and contracts so each business area follows identical validation,
persistence, pagination, error, and serialization rules.

### Domain primitives

Add database-independent types for:

- Opaque entity identifiers with table-specific wrappers.
- `Money` containing checked non-negative minor units and an uppercase ISO currency.
- Positive decimal `Quantity`, limited to three fractional digits.
- Archive state and timestamps.
- Normalized VIN and registration lookup values.
- Page limits, typed collection filters, opaque cursors, and paginated results.
- Shared validation errors with field paths and stable machine-readable codes.

Construction must enforce invariants. Redacted or custom `Debug` implementations must prevent
accidental exposure of future sensitive values.

### Repository and service contracts

- Define common repository error categories: conflict, not found where needed for conditional
  operations, unavailable, and corrupt/unexpected data.
- Never collapse unavailable/corrupt persistence into not-found.
- Services translate repository results into domain workflow outcomes.
- Controllers translate workflow outcomes into API responses.
- Repository methods accept typed filters and cursors rather than HTTP query strings.
- Surreal adapters centralize record-ID parsing, response extraction, query error classification,
  and cursor tuple handling.

### API foundation types

Define shared DTO modules for IDs, timestamps, money, quantities, pagination envelopes, field
errors, and error envelopes. DTOs cannot derive directly from SurrealDB row structs.

### Configuration

Add validated business settings for default currency and collection limits. Default currency is
`EUR`; reject malformed codes and limits outside the documented bounds at startup without printing
unrelated configuration values.

### Acceptance criteria

- [ ] Domain primitives have no Loco, Axum, Tera, or SurrealDB dependency.
- [ ] IDs, money, quantity, normalization, archive state, and pagination enforce invariants.
- [ ] Checked arithmetic and the rounding rule have boundary tests.
- [ ] Repository, service, and HTTP error categories remain separate.
- [ ] SurrealDB types do not appear in public DTOs or repository contracts.
- [ ] Cursor decoding detects tampering, malformed data, and filter mismatch.
- [ ] `docs/architecture.md` describes the expanded module boundaries.

### Verification

```bash
cargo check
cargo test domain
cargo test pagination
cargo test error_mapping
```

---

## Issue 5 — Define customers and vehicles in the desired schema

- **Priority:** High
- **Dependencies:** Issue 4
- **Blocks:** Issues 9 and 11

### Objective

Define strict customer and vehicle schema files, constraints, references, and indexes supporting
fast workshop lookup while preserving user-facing values.

### `customer` schema

| Field | Requirement |
| --- | --- |
| `display_name` | Required trimmed string, 1–160 characters. |
| `display_name_normalized` | Required case-folded search value derived by the application. |
| `email` | Optional trimmed email, maximum 254 characters. |
| `email_normalized` | Optional ASCII-lowercase lookup value. Not globally unique. |
| `phone` | Optional display phone, maximum 40 characters. |
| `phone_normalized` | Optional digits plus leading `+` lookup value. |
| `address` | Optional strict object with line 1, line 2, postal code, city, and two-letter country code. |
| `notes` | Optional workshop notes with a documented maximum length. |
| timestamps | `created_at`, `updated_at`, optional `archived_at`. |

Index normalized name, email, phone, and archive state. Customer search is case-insensitive and
matches the documented normalized values; it must not silently merge duplicates.

### `vehicle` schema

| Field | Requirement |
| --- | --- |
| `customer` | Required `record<customer>` reference with delete rejection. |
| `make` / `model` | Required trimmed strings with normalized search companions. |
| `year` | Optional integer in a documented plausible range, not later than next calendar year. |
| `registration` | Optional preserved display value. |
| `registration_normalized` | Optional unique normalized lookup value. |
| `vin` | Optional preserved display value. |
| `vin_normalized` | Optional unique 17-character normalized value with VIN character validation. |
| `current_mileage` | Optional non-negative integer. |
| `engine_type` | Optional trimmed descriptive string. |
| `notes` | Optional workshop notes. |
| timestamps | `created_at`, `updated_at`, optional `archived_at`. |

Use indexes for customer navigation, registration/VIN lookup, make/model search, and archive state.
Uniqueness applies only to present normalized identifiers. Empty strings must be stored as `NONE`,
not as empty indexed values.

### Ownership and deletion

- A vehicle has exactly one current customer.
- Reassignment updates that reference; this milestone does not reconstruct previous ownership.
- A customer or vehicle referenced by historical records cannot be deleted.
- Archiving a customer does not automatically archive vehicles, but blocks new vehicle assignment
  until the customer is restored.

### Acceptance criteria

- [ ] Both tables are schemafull with explicit fields and timestamps.
- [ ] Customer and vehicle normalization is deterministic and tested.
- [ ] Present VINs and registrations are unique at database level.
- [ ] Multiple `NONE` identifiers are allowed.
- [ ] Vehicle-to-customer references reject parent deletion.
- [ ] Required lookup and navigation indexes exist.
- [ ] Archiving retains all records and relationships.

### Verification

```bash
surrealkit test --suite 'customers*'
surrealkit test --suite 'vehicles*'
cargo test customer_model
cargo test vehicle_model
```

---

## Issue 6 — Define interventions, service history, and line items

- **Priority:** High
- **Dependencies:** Issue 4
- **Blocks:** Issues 9 and 12

### Objective

Define the durable service-history schema, including intervention state, chronology, recorded
mileage, workshop narrative, and cost/revenue line items.

### `intervention` schema

- Required vehicle reference with deletion rejection.
- Required `service_date` and status enum: `draft`, `completed`, or `cancelled`.
- Optional non-negative `mileage` representing the odometer recorded for this intervention.
- Optional bounded text for customer-reported problem, diagnostics, performed work,
  recommendations, and general notes.
- `created_at`, `updated_at`, optional `completed_at`, and optional `cancelled_at`.
- Indexes for vehicle/service-date chronology, status, and recent-work queries.

The API will sort service history by `service_date DESC, created_at DESC, id DESC`. Identical dates
must therefore remain deterministic.

### `intervention_line` schema

- Required intervention reference with delete rejection.
- Category enum: `labour`, `part`, `material`, or `other`.
- Required description, positive quantity, bounded unit label, non-negative unit-price money,
  optional non-negative unit-cost money, and persisted calculated totals.
- Line currency must match the intervention's configured financial currency.
- Stable `position` integer unique within the parent for explicit display ordering.
- Creation and update timestamps.

### State and historical integrity

- Draft interventions may change fields and lines.
- Completion validates required workshop content, timestamps the transition, and freezes ordinary
  edits and line mutations.
- Cancellation is allowed from draft, records its timestamp, and preserves the record.
- Completed and cancelled records cannot return to draft in this milestone.
- New intervention mileage cannot be less than the latest earlier non-cancelled mileage for the
  same vehicle. Backdated records are validated against neighboring chronological records.
- Updating `vehicle.current_mileage` must never rewrite historical intervention mileage.

### Acceptance criteria

- [ ] Intervention and line tables are schemafull.
- [ ] Vehicle deletion is rejected while interventions reference it.
- [ ] State values and timestamp combinations are constrained.
- [ ] Line category, quantity, currency, positions, and totals are validated.
- [ ] Service-history indexes support the required deterministic query.
- [ ] Completed/cancelled records remain durable.
- [ ] Mileage consistency has current, backdated, equal, and invalid regression tests.

### Verification

```bash
surrealkit test --suite 'interventions*'
cargo test intervention_model
cargo test service_history
cargo test intervention_line
```

---

## Issue 7 — Define technical knowledge and attachment metadata

- **Priority:** Medium
- **Dependencies:** Issue 4
- **Blocks:** Issues 9 and 13

### Objective

Define searchable reusable technical knowledge and metadata records that a later storage milestone
can connect to actual files without introducing binary or AI infrastructure now.

### `technical_note` schema

- Required title and body with documented length limits.
- Normalized tag array with per-tag and count limits.
- Optional vehicle and source-intervention references with deletion rejection while present.
- Optional make, model, and engine context stored as display values plus normalized search values.
- Creation/update timestamps and optional archive timestamp.
- A shared analyzer suitable for case-insensitive workshop terms.
- Separate full-text indexes for title and body because SurrealDB full-text indexes operate on one
  field each.
- Conventional indexes for tags, make/model/engine context, source records, and archive state.

Search combines full-text relevance with exact structured filters. It must remain useful without
embeddings or external services.

### `attachment` metadata schema

- Exactly one owner: vehicle or intervention. Reject neither-owner and both-owner records.
- Required display name and supported media type.
- Optional non-negative byte size and caption.
- Storage state fixed to `metadata_only` for records created in this milestone.
- Creation/update timestamps.
- Owner and storage-state indexes.

Do not add binary fields, bucket definitions, object locations, checksums that imply uploaded
content, multipart handlers, or signed URLs. The later storage milestone will migrate these records
when it defines the actual storage lifecycle.

### Acceptance criteria

- [ ] Technical notes support full-text and structured searches.
- [ ] Search indexes use syntax compatible with the pinned SurrealDB version.
- [ ] Notes can reference a vehicle and/or source intervention without depending on them.
- [ ] Notes archive without being deleted.
- [ ] Attachment metadata has exactly one supported owner.
- [ ] Attachment creation does not claim binary content exists.
- [ ] No vector, embedding, agent, or binary-storage dependency is added.

### Verification

```bash
surrealkit test --suite 'technical-notes*'
surrealkit test --suite 'attachments*'
cargo test technical_note
cargo test attachment_metadata
```

---

## Issue 8 — Define invoices, invoice lines, and payments

- **Priority:** High
- **Dependencies:** Issue 4
- **Blocks:** Issues 9 and 14

### Objective

Define a tax-neutral, internally consistent financial schema that supports draft/issued invoices,
immutable line snapshots, sequential issue numbers, and derived payment status.

### `invoice` schema

- Required customer reference; optional vehicle and intervention references.
- Status enum `draft`, `issued`, or `void`.
- Required currency, initially defaulted by application configuration to `EUR`.
- Optional issue number, issue date, due date, customer display snapshot, billing-address snapshot,
  notes, and void reason.
- Persisted subtotal and total in minor units. They are equal while tax is outside scope.
- Creation/update timestamps plus optional `issued_at` and `voided_at`.
- Unique optional final number and indexes for customer, vehicle, intervention, status, issue date,
  and outstanding-invoice listing.

Drafts use their opaque record ID and have no final number. Issuing allocates a monotonically
increasing database sequence value and formats `YYYY-NNNNN` using the UTC issue year. Sequence gaps
are acceptable after failed or cancelled operations; values are never reused.

### `invoice_line` schema

- Required invoice reference with deletion rejection.
- Required description, positive quantity, unit label, unit price, persisted line total, and stable
  position.
- Optional source intervention-line reference for traceability; all displayed financial values are
  snapshots and must not change with the source.
- Currency must match the invoice.

### `payment` schema

- Required invoice reference, positive amount, matching currency, received timestamp, method enum
  `cash`, `bank_transfer`, `card`, or `other`, optional reference, and optional notes.
- Creation timestamp and required `created_by` reference to the authenticated application user.
  The domain exposes this as a `UserId`; only the SurrealDB adapter handles `record<user>`.
- No update/delete API in this milestone. Corrections require an explicit compensating policy in a
  later approved issue; implementation must not silently rewrite a recorded payment.

### Financial invariants

- Draft lines are editable; issued lines and customer/billing snapshots are immutable.
- Issuing and number allocation happen in one service workflow with database transaction
  protection for invoice changes. Sequence gaps remain possible by design.
- Payment creation and outstanding-balance validation happen atomically.
- Sum of payments cannot exceed invoice total and no payment may target a draft invoice.
- `unpaid`, `partially_paid`, and `paid` are derived from total payments; they are not independently
  writable fields.
- Void is allowed only when documented invariants are satisfied; paid invoices cannot be voided
  without a later correction/refund policy.

### Acceptance criteria

- [ ] Invoice, line, and payment tables are schemafull and indexed.
- [ ] Draft, issued, and void field combinations are validated.
- [ ] Final invoice numbers are unique, immutable, and assigned only at issue time.
- [ ] Lines and payments use the invoice currency.
- [ ] Issued financial snapshots are immutable.
- [ ] Payment status is derived and overpayment is rejected.
- [ ] No VAT, tax reporting, payment provider, or legal invoice behavior is implied.

### Verification

```bash
surrealkit test --suite 'invoices*'
surrealkit test --suite 'payments*'
cargo test invoice_model
cargo test payment_model
cargo test money
```

---

## Issue 9 — Generate and verify the initial core-domain rollout

- **Priority:** High
- **Dependencies:** Issues 5–8
- **Blocks:** Issues 11–14

### Objective

Generate one reviewed rollout that introduces the complete core-domain schema and prove it is safe
for both clean databases and databases already containing authentication data.

### Rollout generation

- Begin from the committed authentication baseline snapshots.
- Generate a clearly named timestamped rollout after all business schema files are present.
- Review every generated step. The start phase may add definitions; it must not alter/remove
  authentication definitions or data.
- The initial rollout should have no destructive contract step unless SurrealKit requires metadata
  reconciliation that is reviewed and proven harmless.
- Commit the rollout manifest and resulting schema/catalog snapshots together.

### Verification matrix

Run the rollout against:

1. A clean database initialized from all desired schema files.
2. An existing authentication-only baseline with representative records.
3. A database with the rollout already completed, proving status is stable and no step reapplies.
4. A disposable database rolled back after start.
5. A deliberately drifted database, which must fail before unsafe changes.

Inspect tables, fields, analyzers, indexes, reference deletion policies, and the invoice sequence.
Execute representative customer navigation, vehicle service-history, technical search, invoice
total, and payment-status queries.

### Data-preservation proof

Capture logical authentication fixture projections and counts before and after rollout start,
rollback, and completion. Compare every non-volatile value. SurrealKit metadata additions may
change catalog metadata but not authentication application records.

### Acceptance criteria

- [ ] One reviewed initial business rollout is committed.
- [ ] Manifest and snapshots are generated from committed schema files.
- [ ] Start is additive and authentication-safe.
- [ ] Clean and existing databases reach the same desired catalog.
- [ ] Re-running status/application does not reapply completed steps.
- [ ] Rollback removes only rollout-owned business definitions and metadata transitions.
- [ ] Drift blocks the rollout with an actionable, secret-free error.
- [ ] Representative relationships, constraints, indexes, and queries pass.

### Verification

```bash
surrealkit rollout lint <core-domain-rollout>
surrealkit rollout start <core-domain-rollout>
surrealkit rollout status
surrealkit test
cargo test core_domain_rollout
```

---

## Issue 10 — Build the authenticated `/api/v1` foundation

- **Priority:** High
- **Dependencies:** Issues 3 and 4
- **Blocks:** Issues 11–14

### Objective

Create the shared authenticated JSON transport layer, DTO conventions, cursor pagination, CSRF
enforcement, body limits, and stable error responses used by every domain controller.

### Routing and security

- Add an `/api/v1` route composition module without introducing a second web framework.
- Every registered API route must appear in `ROUTE_ACCESS_POLICY` as authenticated.
- Require `CurrentUser` in every handler.
- Extend the existing CSRF extraction/service so JSON unsafe methods validate the same origin,
  action, expiry, and session binding used by forms and HTMX.
- Reject unsafe requests without a valid CSRF token before invoking a service.
- Apply JSON content-type and per-route body-size limits.
- Return `Cache-Control: no-store` for user-specific business responses unless a later explicit
  cache design supersedes it.

### DTO and error contracts

- Implement the shared response shapes defined above.
- Preserve safe validation field paths and user input rules without exposing internal field names.
- Add a correlation ID to 500/503 errors and secret-safe structured logs.
- Map stale authentication consistently with the existing cookie-clearing behavior where
  applicable.

### Pagination

- Create authenticated opaque cursors containing version, resource kind, filter fingerprint, and
  final sort tuple.
- Sign or MAC cursors with an application secret distinct from JWT and CSRF secrets, or derive a
  purpose-separated key through an approved cryptographic construction.
- Validate default/max limits and reject cursors reused with different filters.
- Never expose raw SurrealQL, a serialized `RecordId`, or database credentials in a cursor.

### Architecture

- Controllers parse DTOs and select responses only.
- Services own validation beyond syntax, state transitions, and transaction boundaries.
- Repository contracts own persistence capabilities; adapters own SurrealQL.
- Add shared test helpers for authenticated JSON requests and CSRF headers.

### Acceptance criteria

- [ ] `/api/v1` routes compose cleanly with existing Loco routes.
- [ ] Every route is covered by the access-policy regression test.
- [ ] Unsafe requests require valid session-bound CSRF.
- [ ] DTOs contain no SurrealDB types.
- [ ] Error codes/status mappings are stable and tested.
- [ ] Pagination limits and cursor integrity are enforced.
- [ ] Persistence failures return opaque 503/500 responses with correlation IDs.
- [ ] Controllers contain no business rules or database queries.

### Verification

```bash
cargo check
cargo loco routes
cargo test api_foundation
cargo test api_authentication
cargo test api_csrf
cargo test cursor
```

---

## Issue 11 — Implement customer and vehicle APIs

- **Priority:** High
- **Dependencies:** Issues 5, 9, and 10
- **Blocks:** Issues 12–14

### Objective

Implement customer and vehicle domain models, repository adapters, services, and API routes with
fast lookup, current ownership, archive behavior, and typed conflicts.

### Repository contracts

Provide create, find-by-ID, update, list/search, archive, and restore capabilities. Vehicle
repositories additionally support normalized VIN/registration lookup, listing by customer, and
atomic current-owner reassignment.

Surreal adapters must use bound values, explicit projections, typed row structs, stable cursor
queries, and conflict classification for unique indexes.

### Service behavior

- Validate and normalize fields before writes while preserving display values.
- Treat duplicate normalized VIN/registration as `conflict` without exposing the existing record
  unless the caller separately retrieves it.
- Reject new vehicle assignment/reassignment to an archived customer.
- Allow reading historical vehicles belonging to archived customers.
- Reassignment changes current ownership only and leaves interventions and invoice snapshots
  untouched.
- Archive/restore operations are idempotent and update timestamps consistently.

### Routes and filters

Implement the customer and vehicle routes in the route inventory. Collection filters include text
query, archive state, customer ID for vehicles, registration, VIN, make, and model as applicable.
Reject unknown filter combinations rather than silently ignoring them.

### Acceptance criteria

- [ ] All customer and vehicle routes require authentication and unsafe-route CSRF.
- [ ] CRUD-style create/read/update plus archive/restore workflows operate through services.
- [ ] List/search endpoints use opaque stable cursors.
- [ ] Duplicate identifiers return 409.
- [ ] Missing records return 404; invalid fields return 422; unavailable DB returns 503.
- [ ] Archived customer assignment is rejected.
- [ ] Vehicle reassignment preserves all historical records.
- [ ] Repository and request tests cover normalization, relationships, pagination, and concurrency.

### Verification

```bash
cargo test customer_repository
cargo test vehicle_repository
cargo test customer_service
cargo test vehicle_service
cargo test customers_api
cargo test vehicles_api
```

---

## Issue 12 — Implement intervention and service-history APIs

- **Priority:** High
- **Dependencies:** Issues 6, 9, 10, and 11
- **Blocks:** Issues 13–15

### Objective

Implement intervention workflows and deterministic vehicle service history while protecting
chronology, mileage, completed records, and line-item totals.

### Repository contracts

Provide intervention create/find/update/list, state transitions, chronological vehicle history,
neighboring-mileage lookup, and line-item create/update/delete/list operations. Expose a transaction
capability sufficient for line mutation plus total recalculation without leaking a SurrealDB
transaction type into services.

### Service behavior

- Reject new interventions for archived vehicles.
- Validate service date and mileage against chronological neighboring records, including backdated
  insert/update cases.
- Keep vehicle current mileage at the maximum applicable recorded mileage; never lower it because
  an older intervention is added.
- Permit field and line changes only while draft.
- Complete only a valid draft and record one completion timestamp.
- Cancel only a draft and preserve it in history with cancelled state clearly represented.
- Apply line mutations and recalculated totals atomically with checked arithmetic.
- Return state conflicts for repeated/invalid transitions.

### Service-history response

Return intervention summaries in the defined deterministic order with recorded mileage, status,
financial summary, and links/IDs needed to fetch detail. Cancelled entries remain visible but are
clearly identified; default filtering may exclude them only if the contract documents an explicit
filter and the frontend can request them.

### Acceptance criteria

- [ ] Intervention, line, transition, and service-history routes match the route inventory.
- [ ] Archived vehicles reject new work but retain readable history.
- [ ] Mileage regression and backdated-neighbor validation are correct.
- [ ] History pagination never duplicates/skips records with identical service dates.
- [ ] Completed/cancelled records reject unsupported edits.
- [ ] Line changes and totals are atomic.
- [ ] Concurrent transitions produce one success and typed conflicts rather than corrupt state.
- [ ] Request/integration tests cover all success and failure paths.

### Verification

```bash
cargo test intervention_repository
cargo test intervention_service
cargo test service_history
cargo test interventions_api
cargo test intervention_concurrency
```

---

## Issue 13 — Implement technical-note and attachment-metadata APIs

- **Priority:** Medium
- **Dependencies:** Issues 7, 9, 10, 11, and 12
- **Blocks:** Issue 15

### Objective

Expose searchable reusable workshop knowledge and honest metadata-only attachment workflows without
adding file storage or AI behavior.

### Technical-note behavior

- Implement create, retrieve, update, list/search, archive, and restore through repositories and
  services.
- Validate optional source vehicle/intervention existence and consistency. When both are present,
  the intervention must belong to the selected vehicle.
- Search full-text title/body plus exact tags and normalized make/model/engine filters.
- Use a deterministic tie-break after relevance so pagination remains stable.
- Archived notes are excluded by default but may be explicitly requested.

### Attachment-metadata behavior

- Implement owner-specific creation/listing plus metadata get/update/delete.
- Derive owner type from the nested route; never accept an arbitrary table name.
- Reject archived vehicle owners, missing owners, and unsupported owner kinds.
- Accept JSON metadata only and reject multipart/binary bodies.
- Keep storage state `metadata_only` and prevent clients from claiming an upload exists.
- Permit deletion only in that state and document that the storage milestone will replace this
  temporary lifecycle.

### Acceptance criteria

- [ ] Technical-note CRUD/search/archive routes are authenticated and CSRF protected as applicable.
- [ ] Structured and full-text filters can be combined.
- [ ] Source relationship conflicts return 409.
- [ ] Search pagination is deterministic.
- [ ] Attachment routes accept only vehicle/intervention owners and JSON metadata.
- [ ] Binary requests and fabricated storage states are rejected.
- [ ] No AI, vector, object-storage, or image-processing dependency appears.

### Verification

```bash
cargo test technical_note_repository
cargo test technical_note_search
cargo test technical_notes_api
cargo test attachment_repository
cargo test attachments_api
```

---

## Issue 14 — Implement invoice and payment APIs

- **Priority:** High
- **Dependencies:** Issues 8–12
- **Blocks:** Issue 15

### Objective

Implement tax-neutral invoice drafting, immutable issue transitions, sequential final numbering,
line calculations, payments, and derived payment status with transaction and concurrency safety.

### Invoice services

- Create drafts for an active customer and optional valid vehicle/intervention relationships.
- If customer, vehicle, and intervention are supplied, validate that vehicle belongs to the current
  customer and intervention belongs to the vehicle at draft time.
- Allow draft header/line edits and recalculate totals atomically.
- Issue a valid non-empty draft by capturing customer/billing snapshots, allocating the number,
  persisting totals, and transitioning state once.
- Reject repeated/concurrent issue attempts with one successful transition and typed conflicts.
- Void only according to the schema invariants; require a bounded reason and retain the number.

### Payment services

- Record payments only against issued, non-void invoices.
- Validate positive amount, currency equality, and received timestamp.
- In one transaction, reload current payments, reject overpayment, insert the payment, and return
  the newly derived status/balance.
- Concurrent payment requests must not both spend the same outstanding balance.
- Expose payment reads but no update/delete route.

### Responses

Invoice DTOs include header snapshots, lines, subtotal/total, paid amount, outstanding amount, and
derived status. They must never expose sequence internals or mutable source records as the issued
snapshot.

### Acceptance criteria

- [ ] Invoice and payment routes match the route inventory.
- [ ] Draft edits recalculate totals with the shared rounding rule.
- [ ] Issue number allocation and transition are concurrency-safe and immutable.
- [ ] Issued snapshots do not change when customer/vehicle/intervention data changes.
- [ ] Currency mismatch, overpayment, draft payment, and invalid relationships are rejected.
- [ ] Payment status and balance are derived consistently.
- [ ] Paid invoices cannot be voided under the milestone's limited correction policy.
- [ ] No tax, legal invoicing, provider, email, or export behavior is introduced.

### Verification

```bash
cargo test invoice_repository
cargo test invoice_service
cargo test invoice_concurrency
cargo test payment_service
cargo test payment_concurrency
cargo test invoices_api
```

---

## Issue 15 — Complete milestone verification and documentation

- **Priority:** Medium
- **Dependencies:** Issues 1–14

### Objective

Prove the entire milestone from a clean checkout and an existing authenticated database, then leave
complete migration, API, schema, and recovery documentation for later frontend and deployment work.

### Documentation deliverables

Create or update:

- `README.md`: prerequisites, pinned tools, clean setup, local schema sync, startup, route listing,
  and complete development gate.
- `docs/migrations.md`: architecture decision, schema ownership, baseline adoption, environment
  policy, planning, manifest review, start/deploy/complete, status, rollback, repair, backup,
  restore rehearsal, drift, troubleshooting, and secret handling.
- `docs/api-v1.md`: authentication/CSRF requirements, DTO conventions, every route, filters,
  pagination, status codes, errors, state machines, and representative requests/responses.
- `docs/architecture.md`: domain modules, repository/service/controller dependencies, transaction
  boundary, and SurrealKit ownership.
- `docs/authentication.md`: replace the old auth-schema task instructions with the verified
  SurrealKit workflow while preserving authentication behavior.

### End-to-end verification

From a clean database:

1. Apply the complete desired schema.
2. Create a user and authenticate.
3. Create customer → vehicle → interventions with lines.
4. Retrieve deterministic service history.
5. Create and find a technical note and attachment metadata.
6. Create an invoice, add lines, issue it, record partial and final payments, and verify status.

From an authentication-only existing database:

1. Inspect and baseline without mutation.
2. Export and checksum a backup.
3. Start the core rollout.
4. Verify existing authentication and new domain operations.
5. Complete the rollout.
6. Confirm authentication fixture projections remain unchanged.

Also rehearse rollback before completion and restore the export to an isolated database.

### Security and scope audit

- Enumerate Loco routes and compare them with `ROUTE_ACCESS_POLICY` and API documentation.
- Prove all business routes reject unauthenticated requests.
- Prove all unsafe business routes reject missing, expired, wrong-action, wrong-session, and
  wrong-origin CSRF tokens.
- Search logs, fixtures, snapshots, errors, and docs for credentials and sensitive auth data.
- Confirm no deferred frontend, binary storage, calendar, inventory, VAT/legal, payment-provider,
  email, role, multi-tenant, vector, or AI behavior was introduced.

### Acceptance criteria

- [ ] A clean checkout can initialize the database and run the application from documentation.
- [ ] An existing authentication database can adopt and complete the rollout without data loss.
- [ ] Rollback and isolated restore procedures have been exercised.
- [ ] Every route, DTO, filter, cursor, status, and error code is documented.
- [ ] Customer → vehicle → service-history and invoice → payment flows pass end to end.
- [ ] Authentication and CSRF boundaries cover every route.
- [ ] Full-text knowledge search and metadata-only attachments behave as documented.
- [ ] The complete automated gate passes with no secret leakage.
- [ ] Deferred scope remains absent.

### Verification

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets --all-features -- -D warnings
surrealkit test
surrealkit rollout lint <each-committed-rollout>
cargo test
cargo loco routes
cargo loco task
```

## Milestone completion checklist

- [ ] All 15 Linear issues are complete with their declared dependencies satisfied.
- [ ] SurrealKit is the only schema authority and the legacy auth-schema task is gone.
- [ ] Clean and existing databases follow documented, tested migration paths.
- [ ] Authentication data survives baseline and core rollout operations unchanged.
- [ ] The entire strict core-domain schema and its indexes/references are verified.
- [ ] Authenticated `/api/v1` routes implement the documented contracts.
- [ ] Service history is complete, deterministic, and protected from destructive changes.
- [ ] Invoice totals, numbering, payments, and derived status are concurrency-safe.
- [ ] Migration operations, API contracts, architecture, and recovery are documented.
- [ ] The frontend milestone can consume the API without depending on SurrealDB records or
      inventing backend behavior.
