# Pipauto — Image Storage Milestone

This document is the source of truth for the Linear issues required to complete the sixth
Pipauto milestone, **Implement an Image storing system**.

It upgrades the existing metadata-only attachment capability. It does not authorize unrelated
media-management, AI, sharing, or deployment work.

## Milestone outcome

At the end of this milestone, Pipauto has authenticated stored attachments that:

- Store PDF, JPEG, PNG, WebP, HEIC, and HEIF content in one SurrealDB file bucket.
- Belong to exactly one vehicle, intervention, or technical note.
- Can be uploaded, listed, opened or downloaded, have their display details edited, and be deleted
  through the existing service-oriented application architecture.
- Derive media type, byte size, and SHA-256 from the uploaded bytes rather than trusting browser
  claims.
- Remain private and are delivered only through authenticated application routes.
- Preserve the existing vehicle, intervention, and technical-note lifecycle rules.
- Recover predictably from interrupted bucket and metadata operations without exposing partial
  uploads as stored files.
- Use a persistent mounted-disk bucket outside tests and an isolated memory bucket in tests.
- Have automated schema, domain, persistence, request, browser, accessibility, and recovery
  coverage.

SurrealDB 3.2.1 remains pinned. Its file-bucket feature is experimental and requires the `files`
experimental capability. The implementation must follow the pinned version's
[bucket definition](https://surrealdb.com/docs/learn/schema-management/files/buckets),
[file-value](https://surrealdb.com/docs/learn/schema-management/files/working-with-files), and
[file-function](https://surrealdb.com/docs/reference/query-language/functions/database-functions/file)
contracts rather than assuming stable behavior from another release.

## Out of scope

- External object storage, including S3, Cloudflare R2, or an application-managed filesystem.
- Public files, unauthenticated links, signed URLs, customer-facing sharing, or email delivery.
- File types other than the approved PDF and image set.
- Image resizing, thumbnails, compression, rotation, EXIF extraction, or transcoding.
- OCR, malware scanning, content indexing, automatic analysis, embeddings, or AI behavior.
- Deduplication, attachment quotas, resumable or chunked upload, and byte-range responses.
- Replacing an attachment's bytes in place. Replacement is delete followed by a new upload.
- Changing vehicle, intervention, or technical-note chronology when attachment data changes.
- A background worker that mutates storage automatically or schema execution during startup.

## Linear metadata

Apply the following metadata to every issue created from this document:

| Field | Value |
| --- | --- |
| Team | `VincentDevi-Perso` |
| Project | `Pipauto` |
| Milestone | `Implement an Image storing system` |
| Assignee | Unassigned |
| Cycle | None |
| Due date | None |

Create the issues in the order below and preserve the dependency relationships stated in each
issue. Issue numbers in this document are dependency aliases, not final Linear identifiers. After
creation, replace aliases in Linear with actual blocking/blocked-by relationships.

## Investigated storage decision

### Existing state

Pipauto is one Loco application using Axum, Tera, HTMX, typed services, persistence-neutral
repository contracts, SurrealDB adapters, authenticated JSON routes under `/api/v1`, and separate
authenticated browser controllers. SurrealDB and its Rust SDK are pinned to 3.2.1.

Attachments currently record honest metadata only. The strict `attachment` table permits exactly
one vehicle or intervention owner and stores a display name, declared media type, optional byte
size, optional caption, timestamps, and a fixed `metadata_only` state. `AttachmentService` validates
owner existence and permits mutations only for active vehicles and Draft interventions. JSON and
browser controllers expose create, list, edit, and delete operations but reject binary claims.

The current application has no technical-note attachment ownership, multipart extractor, file
repository, bucket definition, persisted pointer, checksum, content route, persistent bucket
volume, or recovery workflow. The global request limit is 64 KiB. Browser templates explicitly
state that no file exists.

### SurrealDB constraints

- A bucket is database schema and is applied through the reviewed rollout process, never ordinary
  application startup.
- File support requires `SURREAL_CAPS_ALLOW_EXPERIMENTAL=files` or the equivalent server flag.
- A disk backend requires its directory to be present in `SURREAL_BUCKET_FOLDER_ALLOWLIST`.
- `file::put`, `file::get`, `file::head`, `file::delete`, `file::exists`, and `file::list` operate on
  bucket objects separately from attachment table records.
- The application must not assume that an object-store side effect and a record write roll back as
  one atomic unit. Persisted transition states and idempotent compensation are therefore required.
- The WebSocket query path and Axum multipart extraction hold bounded file bytes in memory. The
  25 MiB file limit is also a memory and denial-of-service boundary.
- A logical SurrealDB export cannot be treated as the only backup of a mounted bucket. Records and
  the bucket volume are one recovery unit.

### Selected approach

- Define one `pipauto_attachments` bucket. Non-test environments use the SurrealDB disk backend at
  a dedicated mounted directory; isolated tests define the same logical bucket with a memory
  backend.
- Set bucket permissions to `NONE`. Pipauto's server-side root-authenticated adapter is the only
  access path; users never receive bucket credentials, names, or keys.
- Split metadata persistence and file operations behind `AttachmentRepository` and
  `AttachmentFileStore`. `AttachmentService` is the only component that coordinates them.
- Buffer at most one file per upload, enforce a 25 MiB file limit, and allow only bounded multipart
  overhead for CSRF, display name, caption, and headers.
- Inspect magic bytes using a closed detector implemented and fixture-tested in Pipauto. Do not
  accept filename extensions or multipart `Content-Type` as evidence. Reject empty, truncated,
  ambiguous ISO-BMFF, and AVIF content.
- Generate an opaque cryptographically random bucket key. Do not place the submitted filename,
  owner display value, registration, VIN, note title, or other workshop data in the key.
- Persist SHA-256 as lowercase hexadecimal for integrity diagnostics. It is not a deduplication key
  and is not returned by public DTOs.
- Use `pending`, `stored`, and `deleting` internal states. Normal get/list/content behavior exposes
  only `stored` attachments.
- Remove all legacy `metadata_only` rows in the rollout contract phase after reporting their count
  and satisfying the existing backup/deployment gate. No fake file or empty object is created for
  them.
- Keep existing mutation locks: active vehicles, Draft interventions, and active technical notes
  accept changes. Archived or terminal owners retain readable attachments but no mutation controls.
- Open PDF, JPEG, PNG, and WebP inline through authenticated routes. HEIC and HEIF download by
  default because browser support is inconsistent. Every type also has an explicit download route.

### Rejected alternatives

- **Storing bytes in the attachment record:** rejected because the approved milestone explicitly
  uses SurrealDB file buckets and file pointers.
- **Direct controller queries or file calls:** rejected because controllers must not bypass domain,
  owner, lifecycle, or persistence boundaries.
- **Trusting `Content-Type` or extension:** rejected because those values are caller-controlled and
  can produce unsafe content delivery.
- **Using the original filename as a bucket key:** rejected because it leaks workshop data, permits
  collisions, and complicates safe path handling.
- **A single fixed `stored` write:** rejected because object and record operations can be
  interrupted independently and need resumable cleanup.
- **Keeping legacy rows indefinitely:** rejected by the approved migration decision. They contain no
  bytes and must not be presented as stored attachments.
- **Loading attachment thumbnails in every detail list:** rejected because no transformation
  pipeline exists and full-resolution phone images would create unnecessary bandwidth and memory
  use.
- **Running reconciliation at startup:** rejected because startup and health checks are
  non-mutating and schema/storage repair must remain an explicit operator action.

## Shared attachment contracts

### Domain model

`AttachmentOwner` becomes a closed enum with `Vehicle`, `Intervention`, and `TechnicalNote`
variants. Exactly one owner is persisted and owner type is always derived from a nested route.

A stored attachment contains:

| Field | Contract |
| --- | --- |
| `id` | Stable opaque application attachment identifier. |
| `owner` | Exactly one supported owner. Immutable. |
| `display_name` | Trimmed 1–255 character display value. |
| `media_type` | Server-derived closed enum: PDF, JPEG, PNG, WebP, HEIC, or HEIF. Immutable. |
| `byte_size` | Required derived integer from 1 through 26,214,400 bytes. Immutable. |
| `caption` | Optional trimmed value up to 1,000 characters. |
| `sha256` | Required lowercase 64-character digest. Persistence-private and immutable. |
| `file` | Required pointer in `pipauto_attachments`. Persistence-private and immutable. |
| `storage_state` | Internal `pending`, `stored`, or `deleting`. |
| timestamps | Creation time plus always-updated transition/metadata time. |

Media detection recognizes the complete signatures needed for the approved formats. HEIC/HEIF
detection must inspect ISO-BMFF brands using positive fixtures for accepted brands and negative
fixtures for AVIF and ambiguous generic containers. The detector never guesses.

`display_name` is optional at the HTTP upload boundary. If absent or blank, the service derives it
from the multipart filename after removing path components, control characters, and unsafe empty
values. If neither source yields a valid display name, upload returns validation errors. The
original filename is not separately persisted.

### Repository boundaries

`AttachmentRepository` owns record behavior:

- Reserve a caller-generated attachment ID and immutable file pointer in `pending` state.
- Find internal records by ID, including transition states for recovery.
- Find/list only stored records for ordinary workflows.
- Mark `pending` as `stored` only when derived size and checksum are present.
- Edit display name and caption only while `stored` and the service permits mutation.
- Mark `stored` or recoverable `pending` as `deleting`.
- Delete only a `deleting` record after object deletion succeeds or absence is confirmed.
- List records by state for explicit reconciliation.

`AttachmentFileStore` owns bucket behavior:

- `put_if_absent(pointer, bytes)` without overwrite.
- `get(pointer)` returning bytes or a typed missing-object result.
- `head(pointer)` returning authoritative size and existence information.
- Idempotent `delete(pointer)` where absence is a successful terminal condition.
- Bounded bucket listing for reconciliation, using opaque cursors/prefixes where supported.

The SurrealDB adapters use typed `File` and bytes values or bound query parameters. They never
construct a file literal by interpolating a display filename or request value into SurrealQL.

### Upload and deletion state machine

```text
validated request
      |
      v
reserve pending record
      |
      v
put_if_absent bytes ──failure──> delete/mark deleting compensation
      |
      v
head verifies size
      |
      v
mark stored ──────────failure──> retain recoverable pending + report safe error

stored ──delete request──> deleting ──delete/confirm absent object──> delete record
```

- Owner and lifecycle validation happens before reserving a record and is repeated before the
  final transition where a stale owner state could matter.
- An upload response is successful only after `stored` is persisted.
- `pending` and `deleting` records are absent from owner lists and content routes.
- Immediate compensation is best effort. A failed compensation remains visible to the explicit
  reconciliation task rather than being disguised as success.
- Delete retries resume from `deleting`; a missing bucket object does not block removal of that
  record.
- A missing object for a `stored` record is corruption. Content delivery returns a safe temporary
  failure with a correlation reference, logs only safe identifiers, and reconciliation reports it.

### Owner lifecycle rules

| Owner | List/open/download | Upload/edit/delete |
| --- | --- | --- |
| Active vehicle | Allowed | Allowed |
| Archived vehicle | Allowed | Conflict; restore first |
| Draft intervention on active vehicle | Allowed | Allowed |
| Completed/cancelled intervention | Allowed | Conflict; historical record stays locked |
| Intervention whose vehicle is archived | Allowed | Conflict; restore vehicle first |
| Active technical note | Allowed | Allowed |
| Archived technical note | Allowed | Conflict; restore note first |

Attachment operations never modify a vehicle's current mileage, intervention mileage, service
date, lifecycle timestamps, history order, technical-note archive time, or any invoice snapshot.

### HTTP upload contract

Multipart create requests contain exactly:

- Required `file` part; exactly one occurrence and non-empty content.
- Optional `display_name` text part.
- Optional `caption` text part.
- One existing session-bound CSRF submission: `X-CSRF-Token` header or `_csrf` text part. If both
  are present, they must match, preserving the current API/browser contract.

Unknown text fields, duplicate singleton fields, multiple file parts, nested multipart content,
malformed boundaries, invalid UTF-8 text, and bodies over the configured limits are rejected. File
bytes are never logged, included in validation responses, or preserved in a redisplayed form after
failure. The form retains safe display name and caption where parsing reached them and asks the user
to reselect the file.

The upload extractor authenticates the session before consuming the body, enforces the total body
limit, validates the header/form CSRF submission using the existing session-bound contract, and
exposes a transport value to the controller. Domain validation remains in the service.

### API route inventory

| Method and path | Behavior |
| --- | --- |
| `GET /api/v1/vehicles/{id}/attachments` | List stored vehicle attachments. |
| `POST /api/v1/vehicles/{id}/attachments` | Multipart vehicle upload. |
| `GET /api/v1/interventions/{id}/attachments` | List stored intervention attachments. |
| `POST /api/v1/interventions/{id}/attachments` | Multipart Draft-intervention upload. |
| `GET /api/v1/technical-notes/{id}/attachments` | List stored technical-note attachments. |
| `POST /api/v1/technical-notes/{id}/attachments` | Multipart active-note upload. |
| `GET /api/v1/attachments/{id}` | Return stored metadata and application content URLs. |
| `PATCH /api/v1/attachments/{id}` | JSON update of display name and/or caption only. |
| `DELETE /api/v1/attachments/{id}` | Begin/resume authenticated deletion. |
| `GET /api/v1/attachments/{id}/content` | Inline-capable authenticated content response. |
| `GET /api/v1/attachments/{id}/download` | Forced authenticated download response. |

Attachment DTOs expose owner identifiers, display name, media type, byte size, caption,
`storage_state: "stored"`, timestamps, `content_url`, and `download_url`. They never expose SHA-256,
bucket name, bucket key, file pointer, or internal transition state.

PATCH omits owner, media type, byte size, checksum, pointer, state, and bytes. Supplying unknown
fields is rejected. Existing JSON metadata-create requests are replaced rather than retained as a
second path.

### Browser route inventory

Retain the current vehicle and intervention attachment form/edit/delete route shapes where they do
not conflict with the new content behavior. Add:

- Technical-note attachment create/edit/delete routes nested under `/knowledge/{id}/attachments`.
- `GET /attachments/{id}/content` for inline-capable content.
- `GET /attachments/{id}/download` for forced download.

Upload forms use `multipart/form-data`, include the existing CSRF token, and work without HTMX or
JavaScript. HTMX may replace only the attachment form or owner attachment region. File inputs are
never repopulated after errors. Owner detail lists show display name, type, size, caption, Open,
Download, and lifecycle-appropriate metadata/delete actions without automatically loading the
full-resolution file.

### Content response contract

- All content routes require `CurrentUser`; bucket URLs never appear in HTML or JSON.
- `Content-Type` and `Content-Length` come from persisted, server-derived metadata verified against
  bucket `head` behavior where needed.
- `Content-Disposition` uses a sanitized ASCII fallback plus correctly encoded UTF-8 filename.
- `/download` always uses `attachment`. `/content` uses `inline` for PDF/JPEG/PNG/WebP and
  `attachment` for HEIC/HEIF.
- Responses include `Cache-Control: private, no-store` and `X-Content-Type-Options: nosniff` and
  retain the application's authenticated response hardening.
- Content is returned as one bounded body. Range requests are ignored or receive the documented
  non-range response; partial-content behavior is not invented.
- A missing attachment returns 404. A known stored record with a missing/unreadable object returns
  a safe 503/corruption response and correlation reference, never an empty successful file.

### Configuration and operations

- Add an attachment setting for the fixed 25 MiB maximum and validate it at startup without
  performing schema or object writes.
- Raise the global middleware ceiling to the multipart envelope only after auditing every unsafe
  non-upload route and applying explicit smaller limits to any uncovered route.
- Extend Compose with `SURREAL_CAPS_ALLOW_EXPERIMENTAL=files`, a narrow bucket-folder allowlist, a
  dedicated attachment mount, and a named persistent volume separate from ordinary database data.
- Test application initialization defines the logical bucket with `BACKEND "memory"` before the
  attachment schema. Non-test schema rollouts define the mounted file backend.
- Readiness may inspect database/bucket catalog state but must not create, modify, or delete schema
  or probe objects.
- Backups and restore rehearsals pair the existing logical database export with a consistent copy
  or snapshot of the bucket volume. No retention, encryption, or legal policy is invented here.

### Reconciliation task

Add an explicit Loco maintenance task with dry-run as the default. It reports:

- `pending` records and whether their object exists and has the expected size.
- `deleting` records and whether deletion can be resumed.
- `stored` records whose object is absent or whose size differs.
- Bucket objects with no attachment record.

Apply mode requires an explicit flag and quiesced attachment writes. It may finish safe pending
transitions only when record metadata, object size, and checksum verification succeed; otherwise it
marks/deletes the incomplete record through the normal deleting flow. It may resume deleting rows
and remove confirmed orphan objects. It never deletes a `stored` record merely because its object
is missing and never guesses replacement content. Output contains counts and safe attachment IDs,
not customer data, filenames, bucket credentials, CSRF values, or file bytes.

## Dependency graph

```text
Issue 1 ──→ Issue 2 ──→ Issue 3 ──┬──→ Issue 4 ──┐
                                  ├──→ Issue 5 ──┼──→ Issue 7 ──→ Issue 8
                                  └──→ Issue 6 ──┘
```

Issues 4–6 may proceed in parallel after the shared domain, persistence, and lifecycle behavior is
complete. Hardening begins only after API, existing-owner browser, and technical-note workflows
exist.

---

## Issue 1 — Establish the attachment-storage contract and bucket runtime

- **Priority:** High
- **Dependencies:** Database Migrations and Core Backend; Implement the frontend
- **Blocks:** Issue 2

### Objective

Create the configuration, dependency, runtime, request-limit, and test foundation required for
SurrealDB bucket storage without yet changing attachment records or exposing upload controls.

### Implementation requirements

- Verify the pinned SurrealDB server and Rust SDK 3.2.1 file types, byte bindings, bucket DDL, and
  file functions. Record any experimental behavior relied upon in storage documentation.
- Add Axum multipart support using the repository's dependency-management convention. Do not add a
  second HTTP framework, upload server, or object-store client.
- Introduce validated attachment settings with a fixed default/maximum of 25 MiB and a bounded
  multipart-envelope constant. Invalid or zero limits fail safely without echoing configured data.
- Audit every unsafe browser and API route before increasing the global payload ceiling. Add
  explicit route limits wherever the current global 64 KiB setting is the only protection.
- Configure Compose with the experimental `files` capability, a folder allowlist containing only
  the attachment mount, and a dedicated persistent named volume.
- Define a test helper that creates `pipauto_attachments` with a memory backend. It must remain
  isolated per disposable test database.
- Add a non-mutating capability/catalog check used by rollout verification and safe readiness
  diagnostics. Do not define the bucket or write a probe object on startup.
- Document that bucket permission is `NONE` and all object operations flow through the server-side
  root-authenticated adapter.

### Acceptance criteria

- [ ] Development SurrealDB starts with file support and the narrow mounted attachment directory.
- [ ] Test databases can define an isolated memory bucket without filesystem access.
- [ ] Upload and multipart-envelope limits are typed, documented, and independently tested.
- [ ] Every non-upload unsafe route retains its previous or stricter request-size bound.
- [ ] Startup performs no schema or bucket mutation.
- [ ] No external object-store or image-processing dependency is introduced.

### Verification

```bash
cargo test attachment_settings
cargo test route_body_limits
cargo test surrealdb_bucket_capability
docker-compose config
cargo check --all-targets
```

---

## Issue 2 — Roll out the bucket and stored-attachment schema

- **Priority:** High
- **Dependencies:** Issue 1
- **Blocks:** Issue 3

### Objective

Create the reviewed SurrealDB bucket and attachment schema transition, including three owner types,
internal storage states, required stored-file metadata, and deliberate removal of legacy rows.

### Schema and rollout requirements

- Add the non-test `pipauto_attachments` bucket definition using the mounted file backend and
  `PERMISSIONS NONE`. Keep the environment-specific memory definition in test setup rather than
  committing a disk path into isolated test initialization.
- Extend `attachment` with optional `technical_note`, file pointer, and SHA-256 fields during the
  additive phase. Expand storage-state validation to `metadata_only`, `pending`, `stored`, and
  `deleting` during compatibility.
- Update owner validation to require exactly one of vehicle, intervention, or technical note and
  retain `REFERENCE ON DELETE REJECT` behavior.
- Assert pointer bucket identity, positive bounded byte size, supported media type, lowercase
  SHA-256 format, and the required-field combinations for each internal state.
- Add indexes for technical-note ownership and storage-state reconciliation while retaining owner
  chronology ordering by `created_at DESC, id DESC`.
- Generate a phased rollout compatible with the approved migration lifecycle. The additive phase
  accepts old code/data while enabling new code; the contract phase runs only after compatible code
  and smoke tests.
- Before deletion, report the count of `metadata_only` rows. In the contract phase delete all such
  rows, assert none remain, remove the legacy state, and tighten the final constraints.
- Update schema/catalog snapshots and SurrealKit suites without weakening unrelated tables.

### Acceptance criteria

- [ ] The bucket appears in the database catalog with the correct backend and permissions.
- [ ] Attachment rows accept exactly one of three valid existing owners.
- [ ] Invalid pointer buckets, digests, sizes, media types, states, and state/field combinations fail.
- [ ] Existing metadata-only rows survive the additive phase and are counted then removed only in
      the reviewed contract phase.
- [ ] A failed contract rollout retains the existing rollout recovery guarantees.
- [ ] Clean and existing databases converge on the same final schema and catalog snapshots.

### Verification

```bash
surrealkit test --suite 'attachments*'
cargo test migration
cargo test attachment_schema
cargo test rollout
```

---

## Issue 3 — Implement stored-attachment domain, repository, and lifecycle orchestration

- **Priority:** High
- **Dependencies:** Issue 2
- **Blocks:** Issues 4, 5, and 6

### Objective

Replace metadata-only domain and persistence behavior with validated stored attachments and a
failure-safe service that coordinates records and bucket objects.

### Domain and detection requirements

- Replace metadata-only types and constants with the stored domain contract in this document.
- Add `TechnicalNote` to `AttachmentOwner` and inject `TechnicalNoteRepository` into the service.
- Implement closed byte-signature detection for all approved types, including positive and negative
  ISO-BMFF fixtures. Do not fall back to header, filename, or extension.
- Enforce non-empty content, 25 MiB maximum, display-name/caption limits, derived byte size, and
  SHA-256 calculation using checked conversions.
- Generate attachment IDs and opaque random file keys without `unsafe`, predictable values, or
  workshop data.

### Persistence and service requirements

- Split `AttachmentRepository` and `AttachmentFileStore` as specified in the shared contract and
  implement SurrealDB adapters for both.
- Decode every persisted pointer, owner, state, media type, size, and digest defensively; malformed
  rows return `CorruptData` without panics.
- Implement upload reserve/write/head/finalize behavior and immediate compensation. Never return a
  created attachment before `stored` persists.
- Implement idempotent mark-deleting/object-delete/record-delete behavior and retry from
  `deleting`.
- Enforce active vehicle, Draft intervention on an active vehicle, and active technical-note
  mutations. List/get/content remain available for archived and terminal owners.
- Limit metadata update to display name and caption and repeat current owner-state validation.
- Map bucket-unavailable, missing-object, collision, corrupt-row, validation, not-found, conflict,
  and temporary failures into typed service errors suitable for both transports.
- Add in-memory/fake adapters with failure injection at every state-machine boundary.

### Acceptance criteria

- [ ] Supported bytes become a stored attachment with derived, immutable metadata.
- [ ] Spoofed headers/extensions and unsupported or ambiguous bytes are rejected.
- [ ] Normal lists never expose pending or deleting records.
- [ ] Upload and delete interruption tests leave only recoverable, truthfully represented state.
- [ ] Owner locks match the lifecycle matrix and never rewrite owner chronology or lifecycle fields.
- [ ] Repository and service code contains no controller, template, or raw request dependency.

### Verification

```bash
cargo test attachment_media_detection
cargo test attachment_model
cargo test attachment_repository
cargo test attachment_file_store
cargo test attachment_service
cargo test attachment_failure_injection
```

---

## Issue 4 — Replace attachment APIs with multipart upload and authenticated delivery

- **Priority:** High
- **Dependencies:** Issue 3
- **Blocks:** Issue 7

### Objective

Expose the stored-attachment service through bounded, CSRF-protected multipart uploads, immutable
metadata APIs, and authenticated content/download responses.

### Implementation requirements

- Add a reusable authenticated multipart extractor that validates `CurrentUser`, total size,
  fields, and the existing header/form session-bound CSRF token before returning a transport value.
- Replace vehicle/intervention JSON metadata creation with multipart upload and add technical-note
  list/upload routes. Retain the nested route as the only source of owner type and ID.
- Reject duplicate/unknown fields, multiple files, empty files, malformed multipart, invalid text,
  and oversize requests with stable validation or payload-too-large responses.
- Keep metadata show/list DTOs transport-safe and expose application content/download URLs only.
- Restrict JSON PATCH to display name and caption with unknown-field rejection.
- Add authenticated `/content` and `/download` handlers with the content response contract,
  sanitized disposition filenames, exact lengths, and safe missing/corrupt behavior.
- Keep DELETE CSRF protected and make retry of an interrupted deletion safe.
- Register every route in the auditable access policy and preserve private/no-store response rules.

### Acceptance criteria

- [ ] Every attachment API route rejects missing, invalid, expired, or revoked sessions.
- [ ] Unsafe routes require valid session CSRF, including multipart uploads.
- [ ] Multipart edge cases and 25 MiB boundaries return stable safe responses.
- [ ] API DTOs expose no pointer, key, bucket, checksum, or internal state.
- [ ] Inline/download headers match each approved media type and never trust request metadata.
- [ ] Missing bucket content never returns an empty 200 response.

### Verification

```bash
cargo test attachment_multipart
cargo test attachments_api
cargo test attachment_content_api
cargo test route_access_policy
cargo test csrf
```

---

## Issue 5 — Upgrade vehicle and intervention attachment browser workflows

- **Priority:** High
- **Dependencies:** Issue 3
- **Blocks:** Issue 7

### Objective

Replace metadata-only vehicle and intervention forms with practical stored-file workflows while
retaining progressive enhancement and historical locks.

### Implementation requirements

- Change shared attachment create forms to `multipart/form-data` with one file picker, optional
  display name, optional caption, CSRF, accepted-type guidance, and the 25 MiB limit.
- Remove declared media-type and byte-size inputs. Explain that Pipauto detects them from content.
- On validation/conflict errors preserve safe text, clear the file input, and explicitly ask the
  user to reselect the file. Never echo bytes or a local path.
- Replace every metadata-only warning, badge, action label, and confirmation with accurate stored
  attachment language.
- Vehicle and intervention detail regions list stored metadata with Open and Download links and
  lifecycle-appropriate Edit details/Delete controls. Do not render eager full-file previews.
- Preserve active-vehicle and Draft-intervention mutation rules in full-page and HTMX responses.
  Completed/cancelled intervention details retain readable links but no mutation controls.
- Keep attachment operations independent of service-history ordering and intervention totals.
- Maintain standard POST/Redirect/GET and bounded HTMX swaps with useful progress and error states.

### Acceptance criteria

- [ ] Upload, open, download, edit details, and delete work for active vehicles and Draft jobs.
- [ ] Archived vehicles and terminal jobs expose stored files read-only.
- [ ] Standard HTML and HTMX paths enforce identical auth, CSRF, validation, and owner rules.
- [ ] Phone, tablet, and desktop forms remain touch-friendly without horizontal scrolling.
- [ ] No metadata-only or fabricated upload language remains in implemented owner views.
- [ ] Attachment actions do not change mileage, history chronology, line order, or totals.

### Verification

```bash
cargo test vehicle_attachment_browser
cargo test intervention_attachment_browser
npx playwright test --grep @attachments
npx playwright test --project=no-javascript --grep @attachments
```

---

## Issue 6 — Add technical-note attachment workflows

- **Priority:** Medium
- **Dependencies:** Issue 3
- **Blocks:** Issue 7

### Objective

Add stored supporting files to reusable technical knowledge without changing search relevance,
source relationships, or archive behavior.

### Implementation requirements

- Add technical-note owner mapping to repository projections, service validation, API DTOs,
  presentation models, and route ownership checks.
- Add active-note create and metadata-edit/delete browser routes plus stored list/open/download
  behavior on note detail.
- Reuse the shared upload form and response behavior rather than creating a second attachment UI.
- Active notes expose mutation actions. Archived notes remain readable and expose Open/Download
  only until restored.
- Validate crafted attachment/note route combinations so an attachment owned by another note,
  vehicle, or intervention returns safe not-found behavior.
- Keep technical-note full-text/structured search, tags, make/model/engine context, source links,
  archive state, and timestamps authoritative and unchanged by attachment operations.

### Acceptance criteria

- [ ] Active notes support the complete stored-attachment workflow.
- [ ] Archived notes retain readable content and no mutation controls.
- [ ] Exactly-one-owner and cross-owner route checks cannot be bypassed.
- [ ] Attachment changes do not alter note search results except the note's independently managed
      fields and do not rewrite source relationships.
- [ ] API, standard browser, HTMX, and no-JavaScript behavior share the same service rules.

### Verification

```bash
cargo test technical_note_attachment_repository
cargo test technical_note_attachments_api
cargo test technical_note_attachment_browser
npx playwright test --grep @knowledge-attachments
npx playwright test --project=no-javascript --grep @knowledge-attachments
```

---

## Issue 7 — Harden storage consistency, accessibility, and recovery

- **Priority:** High
- **Dependencies:** Issues 4, 5, and 6
- **Blocks:** Issue 8

### Objective

Prove the complete attachment system remains truthful, recoverable, secure, and usable across
interruption, malformed input, accessibility modes, and supported viewports.

### Recovery and security requirements

- Implement the dry-run-first reconciliation task exactly as defined in the shared contract, with
  an explicit apply flag and a quiesced-writes precondition.
- Exercise pending, stored, deleting, orphan, missing-object, wrong-size, collision, and bucket
  unavailable cases through deterministic failure injection.
- Verify retries never overwrite an existing opaque key, expose partial bytes, or silently discard
  a known stored record with missing content.
- Audit logs, errors, tracing, test artifacts, and task output for file bytes, filenames, customer
  data, credentials, cookies, CSRF values, pointers, and bucket keys.
- Audit filename encoding, header injection resistance, content sniffing, CSP/no-store behavior,
  authentication, CSRF, payload limits, and crafted owner IDs.
- Confirm upload forms and attachment actions are keyboard and screen-reader usable, have visible
  focus, actionable errors, progress text, and 44px touch targets.
- Exercise HTMX, JavaScript-disabled, slow/failed request, phone, tablet, desktop, and zoom behavior.

### Acceptance criteria

- [ ] Dry-run never mutates records or objects; apply performs only documented recoverable actions.
- [ ] Every interrupted upload/delete state has a tested report and recovery outcome.
- [ ] Stored-record corruption is surfaced safely rather than hidden or auto-deleted.
- [ ] No sensitive file or bucket information leaks through any user-visible or diagnostic path.
- [ ] Accessibility and responsive behavior meet the frontend milestone's WCAG 2.2 AA baseline.
- [ ] All owner workflows remain usable without JavaScript.

### Verification

```bash
cargo test attachment_reconciliation
cargo test attachment_security
cargo test attachment_failure_injection
npx playwright test --grep @attachments
npx playwright test --project=no-javascript --grep @attachments
```

---

## Issue 8 — Complete milestone verification and documentation

- **Priority:** Medium
- **Dependencies:** Issue 7
- **Blocks:** None

### Objective

Prove the complete milestone from clean and existing databases, verify persistent storage and
recovery, and leave implementation and operator documentation sufficient for safe use.

### End-to-end verification

From a disposable database and bucket:

1. Apply the complete schema and confirm bucket capability, backend, and permissions.
2. Upload/open/download/edit/delete each supported type across the three owner kinds.
3. Confirm unsupported, spoofed, empty, malformed, and oversized requests fail safely.
4. Complete/cancel an intervention and archive a vehicle/note; confirm existing files remain
   readable and mutation becomes unavailable.
5. Restart SurrealDB and prove mounted-bucket bytes and metadata remain consistent.
6. Apply the milestone to a fixture database containing metadata-only rows; verify the additive
   phase, deletion count, deployment gate, contract deletion, and final schema.
7. Simulate interrupted upload/delete states and run reconciliation in dry-run then apply mode.
8. Back up database records and the bucket volume, restore both into an isolated environment, and
   verify checksums and application access without overwriting a live database.

### Documentation deliverables

- Update `README.md` with local bucket prerequisites, Compose lifecycle, limits, and verification.
- Add or update attachment/storage documentation covering architecture, state machine, routes,
  supported signatures, settings, error handling, and troubleshooting.
- Update migration/operations documentation with the paired database-plus-volume backup and
  isolated restore rehearsal.
- Update API documentation with multipart examples, DTOs, content headers, and safe errors.
- Update `docs/CONTEXT.md` to reflect the approved stored attachment lifecycle and
  deferred features.
- Record the experimental SurrealDB dependency and the exact upgrade revalidation required before
  changing away from 3.2.1.

### Acceptance criteria

- [ ] Clean and existing-database scenarios pass from a clean checkout.
- [ ] Every supported type and owner works through API and browser workflows.
- [ ] Bucket persistence survives restart and paired backup/restore is rehearsed in isolation.
- [ ] Legacy metadata-only rows are reported and removed exactly as approved.
- [ ] Reconciliation, auth, CSRF, limits, lifecycle locks, content headers, and corruption behavior
      are covered by automated or explicitly documented manual verification.
- [ ] Documentation makes no claim that logical database export alone backs up attachment bytes.
- [ ] Product context contains the approved stored-file boundary and no deferred feature is added.

### Final verification

```bash
cargo fmt --check
cargo check --all-targets
cargo test --all-targets
surrealkit test
npx playwright test
./scripts/ci-check
```

## Milestone completion checklist

- [ ] The reviewed rollout defines the bucket and final stored-attachment schema.
- [ ] Legacy metadata-only rows are counted and removed in the contract phase.
- [ ] Vehicle, intervention, and technical-note attachments use one authenticated service boundary.
- [ ] Supported content is detected from bytes and remains within the 25 MiB limit.
- [ ] Pending/stored/deleting transitions and explicit reconciliation are proven.
- [ ] Content and download routes expose no bucket implementation details.
- [ ] Historical and archive lifecycle rules remain intact.
- [ ] Persistent volume backup/restore accompanies database recovery documentation.
- [ ] API, browser, no-JavaScript, accessibility, security, and failure tests pass.
- [ ] Product and developer documentation describe the implemented behavior accurately.
