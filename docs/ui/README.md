
# Pipauto first-release UI design

## Status and purpose

This directory preserves the UI material used to implement the first-release frontend. It does not
record an approval history, and the presence of a document here must not be presented as evidence
that a separate review or approval occurred.

The design is optimized for one authenticated workshop user moving between a phone, tablet, and
desktop. Fast vehicle lookup and accurate service history take priority over reporting or
administrative features.

## How to use this package

The original index linked to shared `sitemap.md`, `user-flows.md`, and `interactions.md` files and
to a `pages/` directory. Those paths are absent from the repository. Do not fabricate their
contents or infer approval history from the broken links.

The seven UI documents present and used by the expanded first-release design are:

1. [Design system](design-system.md)
2. [Authentication, shell, and dashboard](auth-shell-dashboard.md)
3. [Customers and vehicles](customers-vehicles.md)
4. [Interventions and service history](interventions.md)
5. [Calendar](calendar.md)
6. [Technical knowledge](technical-knowledge.md)
7. [Invoices and payments](invoices-payments.md)

The Calendar document describes the implemented read-only Month/Week route. Its entry and slot
interaction notes remain follow-up design boundaries, not current browser behavior.

The implemented browser route inventory and current component/test contracts live in the
[frontend guide](../frontend.md), which is authoritative when these preserved design documents
describe a path or file that is absent.

Page documents use proposed browser routes. The implementation may use route helper names that fit
Loco conventions, but changing a path must update the sitemap, page specification, links, and tests
in the same change. JSON endpoints remain defined by `docs/api-v1.md` when that backend document is
delivered; this package names required capabilities without inventing final wire shapes.

## Product vocabulary

| Term | Meaning in the interface |
| --- | --- |
| Customer | A person or organization that owns or brings in a vehicle. |
| Vehicle | The current customer-owned vehicle record. Previous ownership is not reconstructed. |
| Intervention / job | A repair, maintenance activity, inspection, or other work on a vehicle. Use **Intervention** in headings and **job** only as supporting plain language. |
| Service history | A vehicle's deterministic reverse-chronological sequence of interventions. |
| Line item | Labour, part, material, or other charge/cost recorded on a draft intervention or invoice. |
| Technical note | Searchable reusable workshop knowledge, optionally linked to a vehicle or source intervention. |
| Draft intervention | Editable work record that can be completed or cancelled. |
| Completed intervention | Immutable historical record. |
| Cancelled intervention | Immutable retained record, visibly marked as cancelled. |
| Draft invoice | Editable invoice with no final invoice number. |
| Issued invoice | Immutable numbered invoice that may receive payments. |
| Archived | Retained and normally hidden from active lists; restorable where the backend supports it. |

## Page inventory

| Area | Pages | Primary capability |
| --- | --- | --- |
| Guest | Login; authentication unavailable | Start a protected session or recover safely. |
| Shell | Dashboard; expired session handling | Navigate, launch common work, and sign out. |
| Customers | List; create; detail; edit | Find and maintain customers and their vehicles. |
| Vehicles | List; register; detail; edit/reassign | Find a vehicle and reach its complete service history. |
| Interventions | List; create; detail; edit | Record workshop work, lines, chronology, and status. |
| Calendar | Month; Week | View Draft and Completed interventions by workshop-local date and time. |
| Technical knowledge | List/search; create; detail; edit | Find and preserve reusable workshop knowledge. |
| Invoices | List; create; detail; edit draft | Draft, issue, void, and track payment status. |

Dialogs and sheets are specified inside their owning page rather than counted as independent
routes. They include archive/restore, reassignment, complete/cancel, add/edit line, issue/void,
record payment, filters, and stored-attachment forms.

## Global product decisions

- All workshop routes are authenticated. Login is guest-only; there is no registration, recovery,
  role, permission, or session-management UI.
- Desktop uses a persistent left sidebar. Phones use a five-item bottom bar: Home, Vehicles,
  Calendar, Jobs, and More. More exposes Customers, Knowledge, Invoices, and Sign out.
- Calendar is a read-only projection of interventions. Its approved first-release views are Month
  and Week. Every intervention has a required start and estimated duration, and entries use
  captured customer/vehicle identity. Calendar does not introduce appointments, generic events, or
  scheduling resources.
- Desktop collection results use compact tables. Phone results use cards. Tablet layout chooses the
  form that avoids horizontal scrolling for the available width.
- The dashboard is a navigation and work queue. It may show recent interventions, drafts, and
  outstanding invoices from existing collection capabilities, but no revenue analytics or new
  reporting contract.
- Customer, vehicle, and technical-note removal uses archive/restore. Historical references remain
  readable. Completed/cancelled interventions and issued invoices are read-only.
- Attachments are private stored files owned by a vehicle, intervention, or technical note. Active
  mutable owners expose upload/edit/delete; archived or terminal owners retain Open/Download only.
  Media type and size are server-derived and bucket details are never shown.
- Invoice export is displayed as unavailable explanatory text until an explicit backend export
  capability exists. It is not rendered as an enabled button.

## Scope boundaries

This package excludes attachment thumbnails/transforms/OCR/public sharing, calendar Day/agenda
views, generic events, drag-and-drop, resizing, recurrence, reminders, resource scheduling,
inventory, tax or legal invoice rules, revenue reports, email delivery, payment providers,
refunds/corrections, customer portals, roles, multi-workshop support, vectors, and AI. These
features must not appear as active controls.

## Linear and backend traceability

| Source | Design coverage |
| --- | --- |
| UI wireframe milestone | All documents in this directory: sitemap, flows, page layouts, states, visual language, responsiveness, and accessibility. |
| VIN-40 | Customer/vehicle fields, search, ownership, archive behavior, unique VIN/registration conflicts. |
| VIN-41 | Intervention chronology, mileage, states, line items, totals, and immutable history. |
| VIN-42 | Technical-note search/context and metadata-only vehicle/intervention attachments. |
| VIN-43 | Draft/issued/void invoices, immutable snapshots, payments, and derived payment status. |
| VIN-45 | Authenticated API boundary, CSRF, validation/error envelopes, no-store behavior, and cursor pagination. |
| VIN-46 | Customer/vehicle CRUD-style flows, filters, archive/restore, and reassignment conflicts. |
| VIN-47 | Draft editing, completion/cancellation, deterministic history, mileage conflicts, and totals. |
| VIN-48 | Technical-note search/archive and owner-specific attachment metadata. |
| VIN-49 | Invoice issue/void transitions, payment recording, balances, and concurrency conflicts. |
| Calendar PRD | Month/Week intervention projection, workshop-local navigation, duration/overlap display, and captured customer/vehicle identity. |

## Design acceptance checklist

- Every page in the inventory has a desktop and phone wireframe.
- Every action names its destination, confirmation behavior, and required backend capability.
- Every data page defines populated, empty, loading, success, validation, conflict, not-found,
  expired-session, unavailable, and unexpected-error behavior where applicable.
- Every unsafe action works with HTMX and as a standard form submission with CSRF protection.
- Focus order, visible focus, labels, status announcements, touch targets, and contrast meet the
  rules in the design system.
- No control implies support for a deferred feature.
