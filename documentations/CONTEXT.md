# Pipauto Project Context

## Project purpose

Pipauto is a workshop-oriented web application for Filippo, a professional mechanic who also repairs cars independently. It is intended to give him one reliable place to manage his customers, their vehicles, and the work performed on those vehicles.

The primary objective is to maintain a complete, accurate, and quickly accessible service history for every vehicle. When a customer returns, Filippo should be able to understand the vehicle's history without reconstructing it from memory or scattered records. This includes previous repairs and maintenance, reported symptoms, diagnostic findings, work performed, parts used, costs, payments, recommendations, and supporting notes or files.

The product should favor practical workshop workflows over administrative complexity. It must be fast and comfortable to use on a phone or tablet as well as on a larger screen.

## Users and operating context

The initial product is designed primarily for Filippo. He may use it while speaking with a customer, inspecting a vehicle, performing a repair, or reviewing past work. The interface should therefore minimize unnecessary steps, make important information easy to scan, and remain usable in a workshop environment.

No broader roles, permissions model, multi-workshop organization, or customer-facing portal is defined by the current product brief. These should not be assumed without an explicit later decision.

### Approved authentication boundary

Pipauto uses administrator-provisioned email/password accounts. Every active user has identical
application access. Browser sessions last a fixed 12 hours and require both a valid signed JWT and
an active matching session-registry record in SurrealDB. Logout revokes the registry record, so a
copied JWT cannot be replayed afterward.

Authentication is server-enforced for the workshop shell and all non-public routes. Login is the
only guest workflow; static assets and non-sensitive health endpoints remain public. Unsafe browser
requests require origin-bound, expiring CSRF tokens, including equivalent standard-form and HTMX
behavior. Production authentication requires HTTPS and secure `__Host-` cookies.

Public registration, password recovery, email verification, social login, MFA, API keys, roles,
granular permissions, refresh tokens, remember-me behavior, and a session-management UI are not in
the initial-release authentication scope.

## Core domain

### Customers and vehicles

Pipauto should support customer profiles and allow each customer to have one or more vehicles. A vehicle record should hold the practical identifying and technical information needed for workshop work, including:

- Make and model.
- Year.
- Registration number.
- Vehicle identification number (VIN).
- Mileage.
- Engine type.

Customers and vehicles should be quick to find. A vehicle page should provide direct access to its complete service history.

Customer names use trimmed display values up to 160 characters and separate full-Unicode
case-folded lookup values. Optional email lookups are ASCII-lowercased; optional phone lookups
contain digits with at most one leading `+`. Empty optional text is absent rather than stored as an
empty indexed value. Postal addresses contain required line 1, postal code, city, and uppercase
two-letter country code plus optional line 2. Customer and vehicle workshop notes are limited to
10,000 characters.

Vehicles retain submitted display registration and VIN values while storing separate unique
normalized lookup values. Registrations are ASCII-uppercased and stripped of spaces, hyphens, and
periods; VINs are trimmed, ASCII-uppercased, exactly 17 characters, and exclude `I`, `O`, and `Q`.
Vehicle years range from 1886 through the next calendar year. A vehicle has one current customer;
reassignment does not reconstruct ownership history. Existing relationships survive archiving,
while archived customers cannot receive newly assigned vehicles. Referenced customers and vehicles
cannot be deleted.

### Interventions and service history

An intervention, also referred to as a job, represents a repair, maintenance activity, inspection, or other piece of work performed on a vehicle. Each intervention may record:

- Date and current mileage.
- The customer's description of the problem.
- Diagnostics and problems identified.
- Work performed.
- Parts and materials used.
- Time spent and labour.
- Costs, the amount charged, and payment information.
- Recommendations and work that may be needed later.
- Notes, photos, and documents.

Together, a vehicle's interventions form its service history. Preserving the accuracy and chronology of this history is a central product requirement.

Interventions begin as drafts and may then be completed or cancelled; neither terminal state can
return to draft. Completion requires a performed-work narrative, records its transition time, and
freezes ordinary intervention and line-item edits. Cancellation is only available from draft,
records its transition time, and preserves the job rather than deleting it. Service history is
ordered by service date, creation time, and intervention identifier, all descending, so jobs on the
same date remain deterministic.

An intervention stores the odometer reading observed for that job independently of the vehicle's
current mileage. Non-cancelled readings must remain non-decreasing through the deterministic
service-history chronology; this applies to current and backdated jobs. Changing a vehicle's
current mileage never rewrites a historical intervention reading.

Intervention lines explicitly order labour, parts, materials, and other charges. Each line stores a
positive quantity with up to three fractional digits, its unit label, non-negative unit price,
optional non-negative unit cost, and persisted calculated price and cost totals. Line currency must
match its intervention currency. Lines remain mutable only while their intervention is a draft.

### Technical knowledge

Pipauto should preserve the practical knowledge Filippo develops through his work. Technical notes should be searchable and reusable when he encounters a similar vehicle, engine, or problem. They may cover:

- Model-specific or engine-specific repair instructions and procedures.
- Recurring problems.
- Difficulties encountered during a repair.
- Solutions and workarounds that were successful.
- Special tools, parts, or precautions required.

Technical knowledge may originate from an intervention, but it should be useful beyond a single customer's service history when the same knowledge applies elsewhere.

Technical notes use a required title of up to 200 characters and a required body of up to 50,000
characters. They accept at most 20 normalized, unique tags of up to 64 characters each and may
independently reference a vehicle and/or their source intervention. Optional make, model, and
engine context preserves display text alongside normalized exact-search values. Title and body use
separate case-insensitive full-text indexes; tags and vehicle context remain exact structured
filters. Notes archive without losing their knowledge or source relationships.

The approved attachment-storage milestone replaces metadata-only placeholders with private files
stored in a SurrealDB bucket. Each stored attachment belongs to exactly one vehicle, intervention,
or technical note and has a display name of up to 255 characters, an optional caption of up to
1,000 characters, a server-derived media type and positive byte size, an integrity checksum, and a
persistence-private file pointer. Supported content remains limited to PDF, JPEG, PNG, WebP, HEIC,
and HEIF, with a maximum size of 25 MiB per file. Media type is detected from file bytes rather than
trusted filenames or request headers.

Attachment upload, metadata editing, deletion, opening, and download are authenticated application
workflows; bucket locations are never public. Mutations are allowed only for active vehicles,
Draft interventions on active vehicles, and active technical notes. Attachments remain readable
when a vehicle or note is archived or an intervention becomes terminal, while their owning record's
chronology and lifecycle remain unchanged. Existing metadata-only rows are removed by the reviewed
storage rollout because they contain no binary content. Image transformation, thumbnails, OCR,
content analysis, public sharing, resumable upload, and external object storage are outside the
initial release.

### Finances and invoices

The initial product should provide a straightforward view of the financial side of the work. It should support:

- Labour, parts, and other expenses.
- Amounts charged to customers.
- Paid, partially paid, and unpaid jobs.
- Revenue and costs over a selected period.
- Professional invoice creation and export.
- Invoice numbering.
- Invoice and payment-status tracking.

Invoices begin as unnumbered drafts and may be issued or voided. Issuing snapshots the displayed
customer, billing, line, currency, and total values and allocates a unique final number from a
monotonically increasing database sequence. Final numbers use `YYYY-NNNNN`, based on the UTC issue
year; sequence gaps are expected and values are never reused. Issued financial snapshots and final
numbers are immutable.

Invoice amounts are tax-neutral in the initial release: persisted subtotal and total minor units
are equal. Invoice lines use positive quantities, snapshot their displayed values, and share the
invoice currency. Payments are append-only, positive records against issued invoices, attributed
to the authenticated user, and cannot exceed the outstanding balance. Unpaid, partially paid, and
paid are derived from recorded payments rather than stored as writable state. Invoices with any
recorded payment cannot be voided until a later correction/refund policy is approved.

VAT, tax reporting, jurisdiction-specific legal invoice behavior, payment providers, refunds,
credit notes, and payment correction behavior remain undefined and outside the initial scope.

### Approved shared domain conventions

The application default currency is EUR. Monetary values use checked, non-negative minor units and
an uppercase ISO 4217 currency code. Multiplying a monetary unit price by a positive quantity (up to
three fractional digits) rounds half-up once to the nearest minor unit. This shared arithmetic rule
does not define taxation, invoice totals, or other accounting policy.

Collection workflows default to 25 records and accept validated limits from 1 through 200 records.
Opaque signed cursors preserve deterministic chronology and are bound to the typed filters used to
produce them.

## Initial-release priorities

The first usable version should focus on five areas, in this order of product importance:

1. Customer management.
2. Vehicle management.
3. Repair and maintenance interventions and service history.
4. Searchable technical notes and model-specific knowledge.
5. Basic financial tracking and invoices.

The current high-level delivery sequence is:

1. Establish the project foundation.
2. Add user access and authentication.
3. Implement customer and vehicle backend capabilities.
4. Design the application's UI wireframes.
5. Implement a functional frontend for customers, vehicles, and interventions.
6. Add a basic calendar.
7. Add image storage for vehicles and interventions.

This sequence comes from high-level milestones, not a detailed implementation backlog. It communicates intended direction and may be refined as requirements and dependencies become clearer.

## Future capabilities outside the initial release

The following ideas are explicitly deferred and should not be included in the initial release unless the scope is deliberately changed:

- Sending invoices to customers by email.
- Accepting contactless tap-to-pay payments through a compatible terminal.
- Appointment reminders and broader appointment-planning capabilities beyond the planned basic calendar.
- Inventory and parts management.
- An AI mechanic assistant.

A future AI mechanic assistant could use accumulated service histories and technical notes to surface similar past problems, successful solutions, model-specific procedures, and other relevant experience. The current release should organize information so it remains useful, but it does not need to implement AI-specific behavior or infrastructure.

## Product and experience principles

- Prefer simple, direct workshop workflows.
- Make common actions fast and important information easy to scan.
- Treat the accuracy, completeness, and chronology of service history as critical.
- Keep the interface responsive and practical on phones, tablets, and desktop screens.
- Avoid features and abstractions that are not required by the current scope.
- Use consistent domain language: customer, vehicle, intervention/job, service history, technical note, invoice, and payment.
- Clearly distinguish confirmed requirements from suggestions, hypotheses, and future ideas.
- Ask before expanding the initial-release scope or making consequential product assumptions.

## Approved database migration boundary

Schema execution is an explicit action and is never part of application startup, health checks, or
ordinary web-server restarts. Tests use isolated disposable synchronization; personal development
may explicitly synchronize a disposable or developer-owned database. Shared development, staging,
and production preserve data through reviewed phased rollouts. Production rollout start additionally
requires a successful checksummed logical export stored outside the repository.

Application deployment for a required rollout is allowed only between its successful additive and
contract phases, while its status is `ready_to_complete`, and only after compatible code is ready.
Smoke tests precede the contract phase. Rollout rollback is available before completion; after a
rollout is terminal, recovery uses a new forward rollout or a backup restored into an isolated
database. Restore rehearsals never overwrite the live production database.

## Approved browser architecture boundary

Server-rendered browser controllers are mounted independently from `/api/v1` and call application
services directly rather than making loopback HTTP requests. Authenticated HTML requests share one
presentation-safe context containing the displayed user name, CSRF token, current path, validated
local return path, and HTMX/full-page preference. Unsafe HTML forms are URL-encoded, session-CSRF
protected, and body-limited; API routes retain their JSON extractor and response contract.

All planned first-release browser routes are authenticated and auditable before their pages are
implemented. The approved workshop shell may link planned destinations before their pages are
implemented; those routes return safe unavailable placeholders and retain their owning navigation
area as active. Full-page and HTMX responses share authentication, CSRF, and `no-store` rules.
Browser views receive presentation models only, never persistence rows, credentials, or session
records.
