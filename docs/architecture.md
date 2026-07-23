# Pipauto architecture

Pipauto is one Loco application serving JSON and server-rendered workshop workflows. It uses
cohesive model modules, slim delivery adapters, explicit runtime capabilities, SurrealDB, and
reviewed SurrealKit schema operations.

## Dependency direction

```text
JSON controllers ─┐
                  ├─> model API ─> private model persistence ─> SurrealDB
HTML controllers ─┘       │
                          └─> shared domain values

model values ─> API DTOs / presentation models ─> JSON / templates
initializers ─> ModelContext and explicit infrastructure capabilities
```

Controllers parse and authorize requests, convert delivery DTOs to model inputs, invoke one model
operation, and select a response. They do not import persistence modules, SurrealDB types, or
query text.

## Module boundaries

| Module | Owns | Must not own |
| --- | --- | --- |
| `app` | Loco lifecycle and top-level route composition | Business or persistence rules |
| `routing` | Access classes, classified route groups, generated route inventory | Runtime authentication or handlers |
| `controllers` | HTTP parsing, authentication/CSRF extraction, response selection | SurrealQL, cross-record invariants, totals |
| `domain` | Shared IDs, money, quantity, normalization, validation, pagination, workshop time | HTTP, templates, database rows |
| `models` | Persisted types, inputs, validation, queries, associations, lifecycle operations | HTTP requests, response DTOs, templates |
| `models::<aggregate>::persistence` | Bound SurrealQL, implementation-only `Db*` rows, atomic mutations | HTTP and presentation behavior |
| `database` | Connection, health, migrations, shared SurrealDB safety mechanics | Business workflows |
| `api` | Explicit public JSON DTOs and envelopes | Stored rows or persistence errors |
| `views` | Typed presentation data and Tera rendering | Request parsing or persistence |
| `auth` | Cookies, CSRF, settings, crypto adapters | User/session persistence |
| `initializers` | Runtime composition and shared-store registration | Business decisions |
| `testing` | Hidden persistence-integrity compatibility exports | Production controller dependencies |

The former top-level public `services` and `repositories` modules do not exist. Aggregate
operations and persistence live behind their public model entry points. Persistence modules are
crate-private except for the documentation-hidden customer persistence and repository compatibility
surface used by integration tests. Ordered line and payment modules, attachment file contracts, and
reconciliation types remain public where callers need those model-owned interfaces.

### Controller organization

Controllers are transport-first:

- `controllers/api_v1/` contains versioned JSON composition and domain controllers.
- `controllers/browser/` contains every HTML/HTMX route area and browser-only shared behavior.
- `controllers/health/` contains infrastructure health routes.
- `controllers/shared/` contains HTTP behavior only when JSON and browser adapters use identical
  semantics.

Every browser route area is a directory. Its `mod.rs` declares workflow modules and composes routes;
request handlers, parsing, validation mapping, and rendering live in workflow-named files such as
`payments.rs`, `history.rs`, or `transitions.rs`. Code uses the domain name `technical_notes` while
retaining `/knowledge` and the visible Knowledge label. Neither delivery adapter imports the other.

## Model context and errors

`ModelContext` is cheap to clone and is constructed once from `AppDatabase`, the cursor codec, and
workshop time. It owns the customer, vehicle, intervention, technical-note, invoice, and attachment
persistence adapters plus the attachment file gateway, so business model handles share dependencies
instead of rebuilding graphs. Calendar projections use intervention persistence; authentication is
composed separately with its cryptographic and time capabilities.

Public model operations return `ModelError`:

- field-oriented validation;
- not found;
- conflict;
- temporary unavailability; or
- internal/corrupt-data failure.

Private persistence errors and raw SurrealDB failures do not cross the model boundary.
Controllers map `ModelError` to stable `AppError` HTTP responses.

## Aggregate ownership

- `customer` owns customer validation, search, archive behavior, and `customer.vehicles`.
- `vehicle` owns identifiers, reassignment, searches, archive behavior, `vehicle.customer`, and
  `vehicle.interventions`.
- `intervention` owns scheduling, immutable identity snapshots, mileage chronology, terminal
  transitions, service-history queries, calendar projections, ordered lines, and totals.
- `technical_note` owns reusable-knowledge validation, search scopes, source checks, and archive
  behavior.
- `invoice` owns drafts, ordered snapshot lines, totals, issue numbering, immutable issuance,
  void restrictions, append-only payments, and derived balances.
- `attachment` owns metadata and coordinates the recoverable database-row/bucket-object lifecycle.
- `auth` owns users, session registry rows, login throttles, login/logout, and administrative
  account operations while consuming explicit cryptographic and time capabilities.

API DTOs and presentation models remain separate from these aggregates so private fields never
become output accidentally.

## Transactions and external side effects

A public model operation is the application operation boundary. Private persistence uses one
SurrealDB transaction whenever the postcondition spans records, including intervention
transitions and line totals, invoice issuance and number allocation, and payment balance checks.

Attachment metadata and bucket bytes are intentionally non-atomic. Upload reserves a pending row,
writes without overwrite, verifies bytes, then finalizes. Deletion marks the row, removes or
confirms absence of the object, then removes the row. The explicit reconciliation task reports and
repairs interrupted states; startup and readiness never do so.

Password hashing, JWT signing, clocks, secure randomness, cookies, CSRF, and attachment file
objects remain explicit capabilities rather than hidden callbacks.

## Schema and SurrealKit ownership

Committed definitions under `database/schema/` are the schema source of truth. SurrealKit owns
schema inspection, snapshots, rollout manifests, linting, disposable synchronization, and phased
rollout execution. Application startup, health checks, and normal requests do not change shared or
production schema. Isolated Loco test startup applies the committed schema only to disposable
in-memory databases.

## Delivery and tests

JSON routes remain under `/api/v1`; browser routes render typed presentation models directly from
model results and never call the JSON API over loopback HTTP. Authentication, CSRF, request body
limits, response envelopes, and public route contracts are delivery concerns and remain unchanged
by the model migration.

Request tests verify public JSON and HTML behavior. Model tests verify validation and lifecycle
rules. Integration tests verify transactions, concurrency, SurrealDB mappings, authentication,
attachment recovery, and migration behavior; their hidden persistence access is not a production
application boundary.
