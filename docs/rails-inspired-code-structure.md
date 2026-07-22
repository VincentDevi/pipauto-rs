# Proposed Rails-inspired code structure

> **Status: proposed target architecture.** This document describes where Pipauto intends to
> move. It does not describe the current implementation, and none of the structural changes below
> should be assumed complete until the corresponding code has been migrated and verified.

## Purpose

Pipauto should follow the application-structure principles documented by the
[Ruby on Rails 8.1 Guides](https://guides.rubyonrails.org/) and reflected in
[The Loco Guide](https://loco.rs/docs/getting-started/guide/): convention over configuration,
cohesive models, slim controllers, and model-owned data behavior.

Pipauto will retain Loco, Rust, SurrealDB, and SurrealKit. The goal is therefore Rails-inspired
application structure, not literal compatibility with Rails' Active Record implementation or
Loco's SeaORM generator. The existing differences are inventoried separately in
[Model and repository differences from the Loco guide](loco-model-repository-differences.md).

This change is structural. It must not alter the approved product behavior in
`docs/CONTEXT.md`, public HTTP contracts, stored data, authorization, service-history
chronology, or financial invariants.

## Governing principles

### Convention over configuration

Every persisted business concept should use the same predictable structure and vocabulary. A
developer looking for customer behavior should begin in `models::customer`, not have to decide
whether it belongs in a controller, service, repository contract, or adapter.

Use singular aggregate module names that match the domain terminology: `customer`, `vehicle`,
`intervention`, `technical_note`, `invoice`, and `payment`. A model module owns the complete public
application API for that concept. Backend mechanics may be split into private submodules when the
file would otherwise become difficult to navigate.

### Cohesive, "fat" models

Rails defines the model as the part of MVC responsible for data and business logic. In Pipauto,
that means a model owns:

- its persisted and input types;
- validation and normalization;
- queries and deterministic scopes;
- create and update behavior;
- lifecycle transitions;
- associations to related models; and
- the transaction that enforces a model operation's postconditions.

"Fat model" does not mean one enormous Rust file. A large aggregate may use private modules for
persistence, commands, lines, totals, or queries while exposing one cohesive model API.

### Slim controllers

Controllers remain responsible for HTTP concerns only:

- authentication and CSRF extraction;
- path, query, form, and JSON parsing;
- request body limits;
- mapping request DTOs into model input types;
- calling a model operation; and
- selecting a response, redirect, or rendered view.

Controllers must not contain SurrealQL, enforce cross-record business rules, recalculate totals,
or coordinate a sequence of persistence calls.

### Model-owned persistence with private backend details

Rails Active Record objects combine data, persistence operations, associations, validation, and
domain behavior. SurrealDB does not provide the same generated Active Record API, so Pipauto will
provide a model-owned API and keep its SurrealDB implementation private.

Public code calls a model. Private model persistence code binds SurrealQL, deserializes database
rows, maps record IDs, and controls transactions. Controllers must not depend on private row types
or query helpers.

### Explicit Rust dependencies

Unlike Rails' process-global Active Record connection, Pipauto should continue to pass runtime
dependencies explicitly. A cheap-to-clone `ModelContext` should provide model operations with the
selected `AppDatabase`, cursor codec, workshop timezone, and other shared model-level settings.
The initializer constructs one context and registers it in Loco's shared store.

This preserves Rust's visible dependency flow without recreating one service object and several
repository trait objects per aggregate.

## Current and target flow

```text
Current:
controller -> service -> repository trait -> SurrealDB adapter -> SurrealDB

Target:
controller -> model API -> private model persistence -> SurrealDB
```

The target removes two public indirection layers from ordinary business operations. It does not
move business rules into controllers or expose SurrealDB response types outside model internals.

## Responsibility changes

| Responsibility | Current owner | Target owner |
| --- | --- | --- |
| Persisted business structs and value validation | `models` and `domain` | Owning model module, with shared value objects in `domain` where genuinely reused |
| HTTP request/response mapping | Controllers and `api` | Unchanged: controllers and `api` |
| Workflow command structs | `services` | Owning model module |
| Validation-to-field error mapping | `services` | Owning model module |
| Search normalization and query scopes | Services and repository filters | Owning model module |
| Association loading and cross-model checks | Services coordinating repositories | Model association and command methods |
| Persistence contracts | Public repository traits | Removed for ordinary business models |
| SurrealQL and private database rows | `repositories::surreal` | Private model persistence modules |
| Atomic multi-record mutations | SurrealDB repository adapters | Owning model's private persistence/command module |
| Repository/workflow errors | `RepositoryError` then `WorkflowError` | One model-facing error contract |
| Database connection and selection | `database` | Unchanged |
| Schema and rollout operations | SurrealKit and `database/schema` | Unchanged |
| JSON DTOs and HTML presentation models | `api` and `views` | Unchanged |
| External file-object operations | `AttachmentFileStore` | Retained as an infrastructure gateway |
| Health checks | Database and health repository/service | Infrastructure health API, outside business models |

## Target module structure

Use one singular module per aggregate. Small aggregates may begin in a single file, but once
persistence is absorbed they should use the same directory convention:

```text
src/
├── controllers/
│   ├── customers.rs
│   ├── vehicles.rs
│   └── browser/
├── models/
│   ├── mod.rs
│   ├── context.rs
│   ├── error.rs
│   ├── customer/
│   │   ├── mod.rs
│   │   └── persistence.rs
│   ├── vehicle/
│   │   ├── mod.rs
│   │   └── persistence.rs
│   ├── intervention/
│   │   ├── mod.rs
│   │   ├── line.rs
│   │   └── persistence.rs
│   └── invoice/
│       ├── mod.rs
│       ├── line.rs
│       ├── payment.rs
│       └── persistence.rs
├── database/
│   ├── client.rs
│   └── surreal_support.rs
├── api/
└── views/
```

The exact private split follows aggregate complexity, but the public entry point is always the
aggregate's `mod.rs`. A controller must not import `models::<aggregate>::persistence`.

Shared SurrealDB mechanics that are not business behavior—checked response extraction, safe
record-ID construction, and backend-error classification—move from
`repositories::surreal::support` to `database::surreal_support`. Aggregate-specific projections,
queries, and row mappings remain in the owning model's private `persistence` module.

The top-level `services` and `repositories` modules disappear only after every consumer has moved.
They must not be removed as an up-front directory reshuffle.

## Model API convention

### Shared context

`ModelContext` is the common dependency passed to public model operations. It should expose only
model-layer capabilities, not HTTP request state or templates. A representative shape is:

```rust
#[derive(Clone)]
pub struct ModelContext {
    database: AppDatabase,
    cursors: CursorCodec,
    workshop_time: WorkshopTime,
}
```

The fields may remain private, with crate-private accessors for model persistence modules. HTTP
authentication, request headers, Tera, and Axum extractors do not belong in this context.

### Class-like and instance operations

Rust models should expose predictable associated functions for collection/creation operations and
instance methods for behavior on a loaded record. For example:

```rust
impl Customer {
    pub async fn create(
        context: &ModelContext,
        input: NewCustomer,
    ) -> Result<Self, ModelError>;

    pub async fn find(
        context: &ModelContext,
        id: &CustomerId,
    ) -> Result<Option<Self>, ModelError>;

    pub async fn search(
        context: &ModelContext,
        query: CustomerQuery,
    ) -> Result<Page<Self>, ModelError>;

    pub async fn update(
        &self,
        context: &ModelContext,
        changes: CustomerChanges,
    ) -> Result<Self, ModelError>;

    pub async fn archive(&self, context: &ModelContext) -> Result<Self, ModelError>;

    pub async fn vehicles(
        &self,
        context: &ModelContext,
        query: VehicleQuery,
    ) -> Result<Page<Vehicle>, ModelError>;
}
```

Names should describe domain behavior rather than generic database operations. Intervention and
invoice APIs therefore keep operations such as `complete`, `cancel`, `issue`, `void`,
`record_payment`, and `move_line` instead of exposing arbitrary status or position updates.

### Input, change, and query types

Move service commands and repository filters into the model that consumes them. Use a consistent
vocabulary:

- `NewCustomer` for validated creation input;
- `CustomerChanges` for explicit partial updates;
- `CustomerQuery` for filtering, ordering, and pagination input; and
- `Customer` for a persisted record.

HTTP request DTOs remain separate because deserialization rules and public field names are
delivery concerns. Private `DbCustomer` types remain separate because SurrealDB projections and
record IDs are persistence concerns.

### Associations

Expose relationships from the owning model rather than through repository-specific helpers. The
standard direction should be discoverable from the domain:

- `customer.vehicles(...)`
- `vehicle.customer(...)`
- `vehicle.interventions(...)`
- `intervention.lines(...)`
- `intervention.technical_notes(...)`
- `invoice.lines(...)`
- `invoice.payments(...)`

Association methods must preserve existing archive filters, deterministic ordering, cursor
binding, snapshot behavior, and not-found semantics. They are not permission to introduce lazy
query behavior in templates; controllers load the data required for a response before rendering.

### Validation and lifecycle behavior

Model constructors and mutation methods own validation, normalization, and lifecycle policy.
Database assertions remain as defense in depth. Callers must not be able to persist an invalid
state by bypassing a service and invoking a lower-level public repository method.

Do not introduce hidden callbacks merely to imitate Rails. Use an explicit model method whenever
ordering, failure handling, chronology, or transaction scope matters. A callback-like private
helper is acceptable only when it is deterministic, local to one model operation, and covered by
that operation's tests.

## Error contract

Replace the current public chain:

```text
RepositoryError -> WorkflowError -> AppError
```

with:

```text
private persistence error -> ModelError -> AppError
```

`ModelError` should preserve the stable application outcomes already exposed by workflows:

- validation errors with field details;
- not found;
- conflict with current state;
- temporary unavailability; and
- internal/corrupt-data failure.

Private persistence errors may retain finer detail for logging and classification but must not
cross the public model boundary. Controllers map `ModelError` to `AppError` and must never expose
raw SurrealDB errors, query text, record contents, or connection details.

## Transactions and complex models

A public model method is the application operation boundary. If its postcondition spans multiple
records, its private persistence implementation owns one SurrealDB transaction.

For interventions, this includes line ordering and totals, lifecycle transitions, identity
snapshots, and mileage chronology. For invoices, this includes line totals, final-number
allocation, immutable issuance snapshots, void restrictions, and payment balance checks.

Large transactional operations may use private command modules, for example
`models::invoice::issue` or `models::intervention::line`, but controllers still call the owning
model's public API. A command module is an implementation detail, not a replacement public service
layer.

## Boundaries that remain separate

### API and presentation

Rails permits models to be rendered directly, but Pipauto will retain explicit JSON DTOs and HTML
presentation models. Loco treats serialized output as the view boundary, and keeping that boundary
prevents credentials, private attachment pointers, normalized lookup fields, and future database
columns from becoming public accidentally.

### Attachment object storage

An attachment metadata row behaves like a model, but a SurrealDB bucket object is an external
side effect with a separately recoverable lifecycle. Keep a narrow file-storage gateway equivalent
to the current `AttachmentFileStore`. The attachment model coordinates reservation, byte storage,
verification, finalization, deletion, and reconciliation without pretending the row and object are
one atomic database operation.

### Infrastructure health

Health checks describe infrastructure availability, not a business aggregate. Keep them under
`database`/infrastructure and expose a small health API to controllers. Do not create a `Health`
Active Record-style model.

### Authentication infrastructure

`User`, `AuthSession`, and login-throttle persistence should become model-owned. Password hashing,
JWT encoding, clocks, secure randomness, cookies, and CSRF remain explicit authentication
infrastructure because they are external capabilities rather than record behavior.

### Schema management

Keep committed `database/schema/` definitions and SurrealKit rollout operations. Rails' important
principle here is reproducible, version-controlled schema evolution; adopting Rails-inspired
models does not justify replacing SurrealKit, applying schemas at application startup, or changing
the reviewed production migration policy.

## Representative target customer flow

After the customer vertical slice is complete:

1. `controllers::customers` parses and authorizes the request and extracts `ModelContext`.
2. The controller converts its request DTO into `NewCustomer`, `CustomerChanges`, or
   `CustomerQuery`.
3. A public `Customer` method validates and normalizes the input.
4. `models::customer::persistence` binds SurrealQL and maps a private `DbCustomer` projection.
5. The public method returns `Customer`, `Page<Customer>`, or `ModelError`.
6. The controller maps the model result into an API DTO or presentation model.

The controller does not know that `DbCustomer` exists. There is no public `CustomerService` or
`CustomerRepository`, and customer behavior remains testable through the public model API.

## Vertical-slice migration

Do not move directories wholesale. Convert one complete business slice at a time and remove old
types only after every caller uses the new model API.

### Phase 1: customers as the reference model

- Introduce `ModelContext` and the shared `ModelError` contract.
- Move customer commands, filters, validation mapping, cursor handling, queries, row conversion,
  and archive behavior into `models::customer`.
- Change JSON and browser customer controllers to call `Customer` methods.
- Preserve current request/response DTOs and customer schema.
- Remove `CustomerService`, `CustomerRepository`, and `SurrealCustomerRepository` only after all
  customer consumers and tests have moved.

The completed customer slice becomes the template for subsequent models. Do not invent a second
pattern for another aggregate unless the customer pattern has a demonstrated limitation.

### Phase 2: vehicles and associations

- Move vehicle validation, searches, reassignment, archive behavior, and persistence into
  `models::vehicle`.
- Add explicit `customer.vehicles` and `vehicle.customer` association APIs.
- Preserve the active-customer assignment rule, unique normalized registration/VIN behavior, and
  the existing opaque cursors.

### Phase 3: technical notes and calendar queries

- Move technical-note lifecycle, search scopes, contextual associations, and persistence into its
  model module.
- Treat calendar as an intervention query/projection, not a separately persisted business model.
- Preserve workshop-timezone conversion, inclusion rules, and deterministic ordering.

### Phase 4: interventions and invoices

- Move service commands and repository mutations into cohesive aggregate modules.
- Keep complex line operations and transactional persistence in private submodules.
- Preserve intervention chronology, terminal-state immutability, mileage constraints, totals,
  issuance snapshots, numbering, append-only payments, and balance checks.
- Migrate these areas only with full transaction and request regression coverage; their repository
  implementations must not be mechanically pasted into public model files.

### Phase 5: authentication and attachments

- Move user, session, throttle, and attachment metadata persistence behind their model APIs.
- Retain cryptographic, clock, random, cookie, CSRF, and attachment-file gateways as explicit
  infrastructure dependencies.
- Preserve attachment reconciliation and failure recovery as explicit operations.

### Phase 6: composition and cleanup

- Simplify the business initializer to construct and register `ModelContext` plus the justified
  infrastructure gateways.
- Remove unused public service and repository modules once repository-wide searches confirm no
  remaining imports.
- Move shared SurrealDB mechanics into `database::surreal_support`.
- Update `docs/architecture.md` and the factual Loco comparison to describe the implementation
  only after the migration is complete.

## Phase checklist

Every vertical slice is complete only when all of the following are true:

- [ ] JSON and browser controllers call the public model API.
- [ ] Model validation and lifecycle behavior match the previous service behavior.
- [ ] Aggregate-specific SurrealQL and `Db*` rows are private to the model.
- [ ] Public HTTP routes, request fields, responses, and statuses are unchanged.
- [ ] Authentication, authorization, CSRF, and body-limit behavior are unchanged.
- [ ] Existing schema, indexes, record IDs, and stored data remain compatible.
- [ ] Pagination filters, cursor binding, and deterministic ordering are unchanged.
- [ ] Cross-record invariants and transactions have regression coverage.
- [ ] Both JSON and server-rendered workflows pass their existing tests.
- [ ] The old service and repository types have no remaining consumers before removal.

## Non-goals

This structural migration does not authorize:

- replacing SurrealDB with a relational database or adding SeaORM;
- changing public APIs, HTML workflows, or route names;
- changing the database schema solely to mimic Rails plural table naming;
- weakening schema assertions because validation moved into models;
- replacing explicit transactional workflows with implicit callbacks;
- removing typed IDs, immutable snapshots, or deterministic cursors;
- changing attachment storage or recovery guarantees; or
- adding product capabilities outside `docs/CONTEXT.md`.

## Completion criteria

The repository follows this target when a developer can begin with a domain model, discover its
validation, persistence operations, lifecycle, queries, and associations through one public
module, and use it from a slim controller without navigating public service or repository layers.
SurrealDB details remain private, infrastructure gateways remain explicit, and every approved
product and data-integrity contract continues to pass its existing tests.
