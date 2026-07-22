# Model and repository differences from the Loco guide

## Purpose and comparison baseline

This document records how Pipauto's implemented model and persistence architecture differs from
the approach demonstrated in [The Loco Guide](https://loco.rs/docs/getting-started/guide/). It is a
description of the current code, not a proposal to change it.

The comparison uses:

- the Loco guide as accessed on 2026-07-22; and
- Pipauto's current `loco-rs` dependency, version `0.16.4`.

The guide is an introductory example, not a mandatory architecture specification. It demonstrates
a SeaORM-backed Active Record style in which generated entities are used directly by controllers
and most business behavior belongs in models. Pipauto still uses Loco for application lifecycle,
routing, configuration, and shared state, but deliberately does not use that persistence pattern.

## Summary

| Concern | Loco guide | Pipauto |
| --- | --- | --- |
| Database and data access | SeaORM and a SQL database | SurrealDB through its Rust SDK and bound SurrealQL |
| Model shape | Generated SeaORM `Model`, `ActiveModel`, and `Entity` types | Hand-written, database-independent domain structs, enums, and value objects |
| Persistence pattern | Active Record | Repository contracts with concrete SurrealDB adapters |
| Business workflow location | Primarily "fat models" | Domain invariants in `models`/`domain`; orchestration in `services` |
| Controller dependency | `AppContext.db` and ORM entities | Services loaded from Loco's shared store |
| Relationships | Generated ORM relations and `find_related` | Explicit repository operations and service-level coordination |
| Persistence representation | Generated entities are application models | Private `Db*` adapter rows converted explicitly to domain models |
| Schema changes | Generated SeaORM migrations, applied by Loco's migrator, then entity regeneration | Committed SurrealQL desired schema and reviewed SurrealKit workflows |
| Write API | Generic insert/update/delete on active models | Workflow-shaped repository methods such as `transition_draft`, `mutate_line`, and `issue` |
| Errors | ORM errors commonly propagate through Loco's error type | Stable repository and workflow error taxonomies hide backend details |
| Testing seam | Database-backed model/request tests in the tutorial | Repository traits allow service tests with in-memory or recording implementations |

## Dependency direction

The guide's representative CRUD path is effectively:

```text
HTTP controller -> generated SeaORM Entity/ActiveModel -> AppContext.db
```

Pipauto's path is:

```text
HTTP controller -> service -> repository trait <- SurrealDB adapter -> SurrealDB
                         \-> domain/model invariants
```

The composition root is the only ordinary application code that knows both sides of the
repository boundary:

- `src/initializers/surrealdb.rs` creates and registers `AppDatabase`.
- `src/initializers/business.rs` creates each concrete `Surreal*Repository`, erases it behind an
  `Arc<dyn *Repository>`, constructs services, and registers those services in Loco's shared store.
- Controllers extract the registered service with `SharedStore<T>` and never extract a database
  connection for business operations.

This is dependency inversion around persistence rather than the guide's direct Active Record use.

## Detailed differences

### 1. SurrealDB replaces SeaORM

The guide assumes SeaORM, installs `sea-orm-cli`, generates SeaORM entities, and runs queries such
as `Entity::find()` or `ActiveModel::insert(&ctx.db)`.

Pipauto has no SeaORM dependency, generated SeaORM entity directory, `ActiveModel`, or `Entity`.
`Cargo.toml` selects `surrealdb`, and the concrete adapters under `src/repositories/surreal/` issue
bound SurrealQL through the SurrealDB SDK.

Consequences:

- SurrealDB record IDs, query responses, transactions, and file values are adapter concerns.
- Loco's `AppContext.db`/SeaORM path is not the business persistence entry point.
- ORM-specific query and relation APIs do not leak into models, services, or controllers.

### 2. Models are hand-written domain models, not generated database entities

The guide generates database-synchronized code under `src/models/_entities/` and warns that those
files should not be edited. Application model extensions sit beside those generated entities.

Pipauto has no `_entities` module. Files under `src/models/` are hand-written and
database-independent. They define application concepts such as `Customer`, `NewCustomer`,
`InterventionStatus`, `EstimatedDuration`, `InvoiceRecord`, and `AttachmentFilePointer`.

As declared in `src/models/mod.rs`, these types must not depend on Loco, Axum, Tera, SurrealDB,
controllers, views, or concrete repositories. They therefore remain usable without a running web
framework or database.

### 3. Domain models are not persistence rows or transport DTOs

In the guide, the generated SeaORM model can be serialized directly as the JSON response. Pipauto
keeps three representations separate:

1. Domain models in `src/models/` represent valid application data.
2. Private adapter rows such as `DbCustomer`, `DbVehicle`, and `DbIntervention` represent the
   selected SurrealDB projection.
3. Controller/view DTOs represent the public JSON or HTML presentation contract.

Surreal adapters implement explicit `TryFrom<Db*>` conversions. Controllers separately implement
domain-to-DTO conversion. As a result, database-only fields and backend types do not become public
API fields by accident, and a public response change does not require changing the stored row
type.

### 4. Persistence is behind application-owned repository traits

The guide calls ORM operations directly. Pipauto defines technology-independent contracts in
`src/repositories/*.rs`, including:

- `CustomerRepository`
- `VehicleRepository`
- `InterventionRepository`
- `CalendarRepository`
- `TechnicalNoteRepository`
- `AttachmentRepository`
- `AttachmentFileStore`
- `InvoiceRepository`
- `UserRepository`
- `AuthSessionRepository`
- `LoginThrottleRepository`
- `HealthRepository`

Their production implementations live under `src/repositories/surreal/`. Contracts accept domain
values, typed filters, page limits, and persistence-neutral cursor tuples; they do not accept HTTP
requests, JSON DTOs, SurrealDB types, or a Loco context.

`AttachmentFileStore` is intentionally separate from `AttachmentRepository`: database metadata
and bucket objects are two side effects with a recoverable, explicitly non-atomic workflow.

### 5. Services replace the guide's direct controller-to-model workflow

Loco promotes "fat models, slim controllers." Pipauto keeps controllers slim but splits the
guide's broad model responsibility:

- `models` and `domain` own valid values and local invariants;
- `services` own application workflows and coordination across repositories; and
- repository adapters own persistence mechanics and atomic database operations.

For example, `CustomerService` validates and normalizes a customer before calling
`CustomerRepository`. `InterventionService` coordinates vehicles, customers, mileage chronology,
and intervention lifecycle. `InvoiceService` coordinates referenced records, issuance, lines, and
payments.

This means "reach for a model" from the guide becomes "call an application service" at Pipauto's
delivery boundaries.

### 6. Repository methods are workflow-shaped, not generic Active Record CRUD

The guide exposes generic `insert`, `update`, `delete`, `find`, and relation-loading operations.
Pipauto repository contracts expose only operations needed by approved workflows. Examples
include:

- `InterventionRepository::update_draft` and `transition_draft` instead of an unrestricted update;
- `InterventionRepository::mutate_line`, which returns the reordered lines and recalculated totals;
- `InvoiceRepository::issue`, `void`, and `record_payment` instead of arbitrary status writes;
- archive/restore operations instead of physical deletion for chronology-sensitive records; and
- `mileage_neighbors` and `vehicle_history` for service-history rules.

The method vocabulary makes lifecycle restrictions visible at the persistence boundary and avoids
offering mutations the product does not allow.

### 7. Cross-model relations are explicit, not generated ORM relations

The guide generates foreign keys and uses ORM relation helpers such as `find_related`. Pipauto has
no generated relation graph. Relationships use typed IDs in domain models and SurrealDB record
links inside adapters.

Services explicitly coordinate repositories when a workflow spans aggregates. The adapter uses
bound record IDs and explicit SurrealQL projections for joins or related lookups. This makes query
shape, existence checks, snapshot behavior, and chronology rules visible in application-owned
code instead of deriving them from ORM metadata.

### 8. IDs are typed, opaque, and persistence-independent

The guide uses a database integer such as `i32` directly in the route and entity API. Pipauto uses
table-specific newtypes including `CustomerId`, `VehicleId`, `InterventionId`, and `InvoiceId` from
`src/domain/id.rs`.

These IDs:

- prevent accidentally passing one entity's ID where another is required;
- contain only a portable opaque key, not SurrealDB's `table:id` syntax;
- validate their format at the boundary; and
- redact their value from `Debug` output.

Only `repositories::surreal::support` constructs or disassembles SurrealDB `RecordId` values and
enforces the expected table name.

### 9. Read and write shapes are explicit

Pipauto commonly distinguishes a validated write model from a stored/read model, for example
`NewCustomer` versus `Customer`, `NewVehicle` versus `Vehicle`, and `NewIntervention` versus
`Intervention`. Restricted updates are represented by service commands or repository mutation
enums rather than by making every persisted field freely settable.

This differs from modifying an `ActiveModel` where every generated active field participates in a
generic update mechanism. It also supports immutable snapshots, generated timestamps, lifecycle
state, calculated totals, and final invoice numbers without treating them as ordinary client
input.

### 10. Adapter row types and projections are private and explicit

Each SurrealDB adapter defines private `Db*` deserialization types matching its query results.
Queries name their selected fields rather than returning a public domain object directly. The
conversion layer validates record table names and maps malformed or unexpected stored data to
`RepositoryError::CorruptData`.

This differs from the guide's generated entity being both the ORM result and the value returned by
the controller. It trades generator convenience for explicit control over stored and exposed
shapes.

### 11. Validation is shared across domain and schema boundaries

The guide's sample `Params::update` copies optional request values into an active model. Pipauto
uses constructors and value objects to trim, normalize, bound, and validate data before a
repository call. Services translate model errors into field-oriented validation errors.

The committed `SCHEMAFULL` SurrealDB definitions repeat critical integrity rules with field
assertions, read-only fields, record-existence checks, and indexes. This is defense in depth:
application validation provides usable errors, while database constraints protect stored history
from invalid writes through any path.

### 12. Error semantics hide persistence technology

Rather than letting SeaORM or SurrealDB errors cross layers, Pipauto uses a small repository error
taxonomy:

- `Conflict`
- `NotFound` for required conditional mutations
- `Unavailable`
- `CorruptData`

An ordinary missing lookup is `Ok(None)`, not an error. Services map repository failures to
`WorkflowError`; controllers map workflow failures to safe HTTP responses. Raw query text,
connection details, and deserialization errors stay inside the adapter.

The Surreal adapter also centralizes checked multi-statement responses, record-ID validation,
cursor conversion, and backend-error classification in `src/repositories/surreal/support.rs`.

### 13. Pagination and filtering belong to repository contracts

The guide lists all records with `Entity::find().all(&ctx.db)`. Pipauto collection repositories
accept typed filters, validated `PageLimit` values, and decoded `CursorTuple` values. Services
normalize search input, authenticate opaque cursors against the complete filter, and encode the
next cursor returned by a repository.

Adapters implement deterministic ordering and fetch one extra row to determine whether another
page exists. HTTP query strings and cursor signatures do not leak into persistence adapters.

### 14. Atomic workflows are implemented in repository adapters

For multi-row operations, the service method defines the application workflow while the concrete
repository owns the database transaction. Intervention line changes and totals, terminal status
transitions, invoice line changes and totals, invoice issuance/number allocation, and payment
balance checks are performed atomically in SurrealDB.

This differs from composing several generic Active Record calls in a controller. It ensures that
the repository method's promised postcondition is the transaction boundary.

### 15. Schema management is independent of model generation and application startup

The guide's model generator creates a SeaORM migration, applies it, and regenerates synchronized
entity code. Pipauto instead uses:

- `database/schema/` as the committed desired-schema source of truth;
- SurrealKit for schema inspection, snapshots, rollout planning, checksums, synchronization, and
  phased execution; and
- `docs/migrations.md` as the operational migration and recovery runbook.

Starting Loco, restarting the server, and calling health endpoints do not apply schema changes.
Production/shared schema execution is a separate reviewed operator action. Domain models are not
regenerated after a migration; developers update schema, adapter mappings, models, services, and
tests deliberately as one contract change.

The one exception is isolated Loco test startup: `src/initializers/business.rs` applies the
committed business schema to its disposable in-memory database.

### 16. Repository implementations are injected and replaceable in tests

Production initializers wrap concrete adapters in `Arc<dyn Repository>`. Services depend only on
those traits. Unit tests can therefore provide purpose-built implementations without booting Loco
or SurrealDB; current examples include recording repositories in service tests and in-memory
attachment metadata/file stores.

This seam is a direct result of the repository layer and is not present in the guide's controller
code, which queries `ctx.db` directly.

### 17. The repository boundary includes backend-specific safety rules

Pipauto's SurrealDB adapters consistently:

- bind data values instead of interpolating user input into queries;
- keep table names and static query structure inside trusted adapter code;
- check every multi-statement response before taking typed results;
- use explicit projections;
- distinguish absence from corrupt result shapes; and
- convert database records to domain values before returning.

Some static transition fragments are selected from closed Rust enums and then interpolated into
query text; caller-provided text remains bound. These conventions replace the safety normally
supplied by a generated ORM query API.

## Representative customer flow

The customer feature shows the complete pattern:

1. `src/controllers/customers.rs` deserializes and bounds the HTTP request, extracts
   `CustomerService`, and maps the result to `CustomerDto`.
2. `src/services/customer.rs` builds validated `Address` and `NewCustomer` values, coordinates
   partial updates, normalizes filters, and manages opaque cursors.
3. `src/repositories/customer.rs` declares the persistence-neutral `CustomerRepository` contract.
4. `src/repositories/surreal/customer.rs` binds SurrealQL values, deserializes private `DbCustomer`
   rows, converts record IDs, and returns `Customer` domain models.
5. `database/schema/business/customer.surql` independently enforces the stored table shape,
   assertions, timestamps, and indexes.

No controller imports a SurrealDB type, no service constructs a SurrealDB record ID, and no domain
model knows how it is serialized in the database.

## What remains conventional Loco

These differences do not mean Pipauto replaces Loco's whole application model. The project still
uses Loco's:

- `Hooks` and application boot lifecycle;
- route registration;
- initializers and `AppContext`;
- shared store for application services;
- environment-aware configuration and tasks; and
- Axum integration exposed through Loco.

The divergence is specifically at the business model/persistence boundary and at the placement of
workflow logic. Loco remains the outer application framework; Pipauto owns the domain, service,
repository, and SurrealDB adapter layers inside it.

## Maintenance rule

When this architecture changes, update this document together with `docs/architecture.md`. When a
product decision changes domain behavior, update `docs/CONTEXT.md` in the same change.
Do not add generated SeaORM entities, direct business queries from controllers, or SurrealDB types
to domain models without recording an explicit replacement for the repository boundary described
here.
