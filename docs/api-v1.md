# Pipauto JSON API v1

The `/api/v1` API is the authenticated backend for Pipauto's workshop workflows. It is an
application API, not a public or third-party integration contract. All responses that contain
user-specific data include `Cache-Control: no-store`.

## Authentication and CSRF

Every route below requires the `pipauto_session` cookie in development or the
`__Host-pipauto_session` cookie in production. The signed JWT and its matching, active SurrealDB
session-registry record must both be valid. Missing, expired, revoked, or otherwise invalid
credentials return `401 unauthenticated` and stale cookies are cleared.

Every `POST`, `PATCH`, and `DELETE` request additionally requires:

- `Content-Type: application/json`;
- the exact configured canonical origin in `Origin` (or a same-origin `Referer` fallback);
- one `X-CSRF-Token` issued in the authenticated workshop shell.

The CSRF token is HMAC-signed and bound to the unsafe action, canonical origin, active session
`jti`, and expiry. Missing, expired, wrong-action, wrong-session, wrong-origin, conflicting, or
duplicate tokens return `403 forbidden` before a business service is invoked. JSON routes do not
accept `_csrf` in the body.

## DTO and collection conventions

- JSON field names are `snake_case`; unknown request fields are rejected.
- Resource identifiers are opaque strings. A client must not construct or parse them.
- Dates are `YYYY-MM-DD`; timestamps are RFC 3339 UTC strings.
- Optional response values are JSON `null`. On `PATCH`, an omitted field means “unchanged” and an
  explicit `null` clears a nullable field.
- Money is `{ "minor_units": 1250, "currency": "EUR" }`. Amounts are checked, non-negative
  integer minor units and currencies are assigned uppercase ISO 4217 codes.
- Quantities are decimal strings, for example `"1.500"`, and must be positive with at most three
  fractional digits. Line totals are persisted after one half-up rounding operation.
- A single resource is `{ "data": RESOURCE }`. A paged collection is
  `{ "data": [RESOURCE], "next_cursor": STRING_OR_NULL }`. Non-paged line, payment, and
  attachment lists use `{ "data": [RESOURCE] }`.
- Paged routes default to `limit=25` and accept `1..=200`. `cursor` is an opaque signed value tied
  to the resource, final deterministic sort tuple, and all filters. A malformed, altered, or
  filter-mismatched cursor returns `422 validation_failed`.
- Archive filters accept `active` (default), `archived`, or `all`.

## Route inventory

### Customers

| Route | Success | Request or filters | Result |
| --- | --- | --- | --- |
| `GET /api/v1/customers` | `200` | `limit`, `cursor`, `q`, `archived` | Customers ordered by creation time then identifier, descending. `q` searches normalized name, email, and phone values. |
| `POST /api/v1/customers` | `200` | Customer create DTO | Creates an active customer. |
| `GET /api/v1/customers/{id}` | `200` | — | Reads active or archived customer. |
| `PATCH /api/v1/customers/{id}` | `200` | Customer patch DTO | Updates supplied fields. |
| `POST /api/v1/customers/{id}/archive` | `200` | `null` | Idempotently archives; references remain intact. |
| `POST /api/v1/customers/{id}/restore` | `200` | `null` | Idempotently restores. |
| `GET /api/v1/customers/{id}/vehicles` | `200` | `limit`, `cursor`, `q`, `archived`, `registration`, `vin`, `make`, `model` | Vehicles whose current customer is `{id}`. Supplying a different `customer_id` is rejected. |

Customer create DTO: `display_name` is required; `email`, `phone`, `address`, and `notes` are
optional. Address is `{line_1, line_2?, postal_code, city, country_code}`. Customer patch accepts
the same fields as optional values. Names are at most 160 characters, notes at most 10,000,
country codes are uppercase two-letter codes, and empty optional text is stored as absent.

Customer response fields are `id`, `display_name`, `email`, `phone`, `address`, `notes`,
`created_at`, `updated_at`, and `archived_at`.

### Vehicles

| Route | Success | Request or filters | Result |
| --- | --- | --- | --- |
| `GET /api/v1/vehicles` | `200` | `limit`, `cursor`, `q`, `archived`, `customer_id`, `registration`, `vin`, `make`, `model` | Deterministic vehicle page. Registration and VIN are exact normalized filters; make/model are normalized exact context filters; `q` is general search. |
| `POST /api/v1/vehicles` | `200` | Vehicle create DTO | Creates a vehicle for an active customer. |
| `GET /api/v1/vehicles/{id}` | `200` | — | Reads active or archived vehicle. |
| `PATCH /api/v1/vehicles/{id}` | `200` | Vehicle patch DTO | Updates fields or current customer; it does not invent ownership history. |
| `POST /api/v1/vehicles/{id}/archive` | `200` | `null` | Idempotently archives. |
| `POST /api/v1/vehicles/{id}/restore` | `200` | `null` | Idempotently restores. |

Vehicle create requires `customer_id`, `make`, and `model`; it accepts nullable `year`,
`registration`, `vin`, `current_mileage`, `engine_type`, and `notes`. Patch accepts the same fields
optionally. Year is 1886 through next calendar year. A VIN is exactly 17 normalized characters and
excludes `I`, `O`, and `Q`. Normalized registrations and VINs are unique.

Vehicle response fields are `id`, `customer_id`, `make`, `model`, `year`, `registration`, `vin`,
`current_mileage`, `engine_type`, `notes`, `created_at`, `updated_at`, and `archived_at`.

### Interventions, lines, and service history

| Route | Success | Request or filters | Result |
| --- | --- | --- | --- |
| `GET /api/v1/interventions` | `200` | `limit`, `cursor`, `vehicle_id`, `status`, `service_date_from`, `service_date_to` | Intervention summaries with calculated totals. Status is `draft`, `completed`, or `cancelled`. |
| `POST /api/v1/interventions` | `201` | Intervention create DTO | Creates a draft intervention. |
| `GET /api/v1/interventions/{id}` | `200` | — | Reads one intervention. |
| `PATCH /api/v1/interventions/{id}` | `200` | Intervention patch DTO | Updates a draft. |
| `POST /api/v1/interventions/{id}/complete` | `200` | `null` | Completes a draft that has performed work. |
| `POST /api/v1/interventions/{id}/cancel` | `200` | `null` | Cancels a draft. |
| `GET /api/v1/vehicles/{id}/service-history` | `200` | `limit`, `cursor`, `status`, `service_date_from`, `service_date_to` | Vehicle history ordered by service date, creation time, and identifier, all descending. |
| `GET /api/v1/interventions/{id}/lines` | `200` | — | Lines ordered by `position`, creation time, and identifier. |
| `POST /api/v1/interventions/{id}/lines` | `201` | Intervention-line DTO | Adds a line to a draft and returns `{line, totals}`. |
| `PATCH /api/v1/interventions/{id}/lines/{line_id}` | `200` | Intervention-line DTO | Replaces a draft line and returns recalculated totals. |
| `DELETE /api/v1/interventions/{id}/lines/{line_id}` | `200` | `null` | Deletes a draft line and returns `{line: null, totals}`. |

Intervention create requires `vehicle_id` and `service_date`; it accepts `mileage`,
`customer_reported_problem`, `diagnostics`, `performed_work`, `recommendations`, `notes`, and
`currency` (default `EUR`). Patch omits `vehicle_id` and makes the other fields optional. Narrative
fields are at most 10,000 characters. Non-cancelled historical mileage cannot regress.

Intervention response fields are `id`, `vehicle_id`, `service_date`, `status`, `mileage`, the five
narrative fields, `currency`, `created_at`, `updated_at`, `completed_at`, `cancelled_at`, and
`links.{detail,lines}`. A history entry is `{intervention, totals:{price,cost}}`.

An intervention-line request is `{category, description, quantity, unit_label,
unit_price_minor, unit_cost_minor, position}`. Category is `labour`, `part`, `material`, or
`other`. Responses add `id`, `intervention_id`, `unit_price`, `unit_cost`, `total_price`,
`total_cost`, `created_at`, and `updated_at`. Currency is inherited from the intervention.

State machine: `draft -> completed` or `draft -> cancelled`. Terminal interventions and their
lines are immutable; intervention records cannot be deleted.

### Technical notes

| Route | Success | Request or filters | Result |
| --- | --- | --- | --- |
| `GET /api/v1/technical-notes` | `200` | `limit`, `cursor`, `q`, `tags`, `make`, `model`, `engine`, `archived` | Full-text title/body search combined with exact normalized context and comma-separated tag filters. |
| `POST /api/v1/technical-notes` | `201` | Technical-note write DTO | Creates an active note. |
| `GET /api/v1/technical-notes/{id}` | `200` | — | Reads active or archived note. |
| `PATCH /api/v1/technical-notes/{id}` | `200` | Technical-note patch DTO | Updates supplied content, source, context, and tags. |
| `POST /api/v1/technical-notes/{id}/archive` | `200` | `null` | Idempotently archives. |
| `POST /api/v1/technical-notes/{id}/restore` | `200` | `null` | Idempotently restores. |

Write requires `title` (200 characters) and `body` (50,000), and accepts `tags` (up to 20 unique
normalized values of 64 characters), `vehicle_id`, `source_intervention_id`, `make`, `model`, and
`engine`. Patch accepts all fields optionally; explicit `null` clears nullable relationships and
context. A source intervention must belong to the referenced vehicle when both are present.

Response fields are `id`, `title`, `body`, `tags`, `vehicle_id`, `source_intervention_id`,
`make`, `model`, `engine`, `created_at`, `updated_at`, and `archived_at`. Each context value is
`{display, normalized}`.

### Attachment metadata

| Route | Success | Request | Result |
| --- | --- | --- | --- |
| `GET /api/v1/vehicles/{id}/attachments` | `200` | — | Vehicle-owned metadata ordered deterministically. |
| `POST /api/v1/vehicles/{id}/attachments` | `201` | Attachment write DTO | Creates vehicle-owned metadata. |
| `GET /api/v1/interventions/{id}/attachments` | `200` | — | Intervention-owned metadata. |
| `POST /api/v1/interventions/{id}/attachments` | `201` | Attachment write DTO | Creates intervention-owned metadata. |
| `GET /api/v1/attachments/{id}` | `200` | — | Reads metadata. |
| `PATCH /api/v1/attachments/{id}` | `200` | Attachment patch DTO | Updates metadata without changing owner or storage state. |
| `DELETE /api/v1/attachments/{id}` | `204` | `null` | Deletes a metadata-only record; response has no body. |

Write is `{display_name, media_type, byte_size?, caption?}`; patch makes every field optional and
allows `null` for `byte_size` and `caption`. Supported media types are `application/pdf`,
`image/jpeg`, `image/png`, `image/webp`, `image/heic`, and `image/heif`. Response fields are `id`,
`owner_type`, `vehicle_id`, `intervention_id`, `display_name`, `media_type`, `byte_size`, `caption`,
`storage_state`, `created_at`, and `updated_at`. `storage_state` is always `metadata_only`; no route
accepts multipart data, binary content, object locations, checksums, or upload claims.

### Invoices, invoice lines, and payments

| Route | Success | Request or filters | Result |
| --- | --- | --- | --- |
| `GET /api/v1/invoices` | `200` | `limit`, `cursor`, `status` | Invoice page; status is `draft`, `issued`, or `void`. |
| `POST /api/v1/invoices` | `201` | Invoice create DTO | Creates an unnumbered draft. |
| `GET /api/v1/invoices/{id}` | `200` | — | Complete invoice view with lines and payments. |
| `PATCH /api/v1/invoices/{id}` | `200` | Invoice patch DTO | Updates a draft. |
| `POST /api/v1/invoices/{id}/issue` | `200` | `{issue_date, due_date?}` | Snapshots and numbers a non-empty draft. |
| `POST /api/v1/invoices/{id}/void` | `200` | `{reason}` | Voids a draft or an unpaid issued invoice. |
| `GET /api/v1/invoices/{id}/lines` | `200` | — | Ordered invoice lines. |
| `POST /api/v1/invoices/{id}/lines` | `201` | Invoice-line DTO | Adds a draft line and returns `{line, subtotal, total}`. |
| `PATCH /api/v1/invoices/{id}/lines/{line_id}` | `200` | Invoice-line DTO | Replaces a draft line and recalculates totals. |
| `DELETE /api/v1/invoices/{id}/lines/{line_id}` | `200` | `null` | Deletes a draft line and returns `{line: null, subtotal, total}`. |
| `GET /api/v1/invoices/{id}/payments` | `200` | — | Append-only payments ordered by receipt time, creation time, and identifier, ascending. |
| `POST /api/v1/invoices/{id}/payments` | `201` | Payment DTO | Records a payment and returns `{payment, invoice}`. |
| `GET /api/v1/payments/{id}` | `200` | — | Reads one payment. |

Invoice create requires `customer_id`; it accepts `vehicle_id`, `intervention_id`, `currency`
(default `EUR`), and `notes`. Patch accepts those fields optionally. Relationships must be
consistent: a vehicle belongs to the customer and an intervention belongs to that vehicle.

Invoice response fields are `id`, relationship IDs, `status`, derived `payment_status`, `currency`,
`number`, `issue_date`, `due_date`, `customer_display_snapshot`, `billing_address_snapshot`,
`notes`, `void_reason`, embedded `lines` and `payments`, `subtotal`, `total`, `paid`, `outstanding`,
`created_at`, `updated_at`, `issued_at`, and `voided_at`. Tax-neutral initial-release totals have
`subtotal == total`.

An invoice-line request is `{source_intervention_line_id?, description, quantity, unit_label,
unit_price_minor, position}`. Responses add IDs, `unit_price`, `line_total`, and timestamps.

A payment request is `{amount_minor, currency, received_at, method, reference?, notes?}`. Method is
`cash`, `bank_transfer`, `card`, or `other`. Response fields are `id`, `invoice_id`, `amount`,
`received_at`, `method`, `reference`, `notes`, and `created_at`. Payments must be positive, use the
invoice currency, target an issued invoice, and not exceed its outstanding balance.

Invoice state machine: `draft -> issued` or `draft -> void`; an unpaid issued invoice may also
become `void`. Issued snapshots, final `YYYY-NNNNN` numbers, and lines are immutable. Payment status
is derived as `unpaid`, `partially_paid`, or `paid`. Payments are append-only, and an invoice with
any payment cannot be voided.

## Status codes and errors

| Status | Error code | Meaning |
| --- | --- | --- |
| `200` | — | Successful read, update, transition, or customer/vehicle creation. |
| `201` | — | Intervention, line, technical note, attachment, invoice, or payment created. |
| `204` | — | Attachment metadata deleted. |
| `400` | `malformed_request` | JSON or query syntax could not be read. |
| `401` | `unauthenticated` | Active authenticated session required. |
| `403` | `forbidden` | CSRF/origin boundary rejected the request. |
| `404` | `not_found` | Resource is absent or not visible through the requested relationship. |
| `409` | `conflict` | Unique constraint, relationship, state transition, chronology, or concurrency conflict. |
| `413` | `malformed_request` | Route-specific request-body limit exceeded. |
| `415` | `malformed_request` | Unsafe route did not receive JSON. |
| `422` | `validation_failed` | DTO, filter, cursor, identifier, or domain value is invalid. |
| `500` | `internal_error` | Opaque internal failure with correlation ID. |
| `503` | `database_unavailable` | Persistence unavailable, with correlation ID. |

Errors are:

```json
{
  "error": {
    "code": "validation_failed",
    "message": "Check the submitted values.",
    "fields": { "display_name": ["Enter a display name."] },
    "correlation_id": null
  }
}
```

Only `500` and `503` set `X-Correlation-ID` and a matching non-null body field. Infrastructure
details, rows, credentials, tokens, password hashes, and session digests are never returned.

## Representative workflow

After signing in through `/login`, obtain the CSRF token from the workshop shell and retain the
session cookie in a protected client cookie jar. For example:

```http
POST /api/v1/customers HTTP/1.1
Content-Type: application/json
Origin: http://localhost:5150
X-CSRF-Token: <session-bound-token>
Cookie: pipauto_session=<signed-session>

{
  "display_name": "Mario Rossi",
  "email": "mario@example.com",
  "address": {
    "line_1": "1 Workshop Lane",
    "postal_code": "1000",
    "city": "Brussels",
    "country_code": "BE"
  }
}
```

```json
{
  "data": {
    "id": "opaque-customer-id",
    "display_name": "Mario Rossi",
    "email": "mario@example.com",
    "phone": null,
    "address": {
      "line_1": "1 Workshop Lane",
      "line_2": null,
      "postal_code": "1000",
      "city": "Brussels",
      "country_code": "BE"
    },
    "notes": null,
    "created_at": "2026-07-19T12:00:00Z",
    "updated_at": "2026-07-19T12:00:00Z",
    "archived_at": null
  }
}
```

Use returned opaque IDs to create a vehicle, intervention and line, complete the intervention,
then read `GET /api/v1/vehicles/{id}/service-history`. Create and search a technical note with
`q`, create attachment metadata, then create an invoice and line, issue it, and post partial and
final payments. The executable request suites perform this complete chain and verify deterministic
history, metadata-only attachment behavior, full-text search, snapshots, balances, and derived
payment status.
