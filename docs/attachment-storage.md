# Attachment storage runtime contract

Pipauto stores attachment bytes in the single private SurrealDB bucket
`pipauto_attachments`. The bucket is schema: application startup and health checks never define,
replace, or remove it and never write probe objects. Tests may explicitly call
`define_attachment_memory_bucket` after selecting a disposable database.

The bucket permission is `NONE`. Browser and API users never operate on bucket values directly;
all object operations go through Pipauto's server-side, root-authenticated SurrealDB adapter.
Bucket names, keys, backend paths, and raw catalog errors are not readiness response data.

## Architecture and ownership

Every attachment belongs to exactly one vehicle, intervention, or technical note. Controllers
derive that owner from the nested route and call the same `AttachmentService`; they do not query
records or buckets directly. The service coordinates two persistence-neutral contracts:

- `AttachmentRepository` reserves and transitions metadata records.
- `AttachmentFileStore` writes, reads, inspects, lists, and idempotently deletes bucket objects.

The SurrealDB adapter binds typed `File` pointers and `Bytes`. The opaque random object key contains
no submitted filename, vehicle registration/VIN, note title, or other workshop data. Public DTOs
and HTML never expose the checksum, bucket, key, pointer, or non-stored transition states.

Record and object operations cannot be one SurrealDB transaction. The service therefore makes
partial work explicit:

```text
validated upload
  -> reserve pending record
  -> put bytes if object is absent
  -> verify object size
  -> recheck owner lifecycle
  -> persist size + SHA-256 and mark stored

stored
  -> mark deleting
  -> delete object or confirm it is absent
  -> delete record
```

Only `stored` rows appear in lists, metadata reads, or content routes. Best-effort compensation is
safe to retry. A stranded `pending` or `deleting` row remains available to reconciliation. A
missing or wrong-sized object for a `stored` row is corruption: content delivery returns a safe
temporary failure and reconciliation retains the record for investigation.

Attachment changes never alter vehicle mileage, intervention chronology or lifecycle timestamps,
technical-note search/source relationships, or invoice snapshots.

## Owner lifecycle

| Owner state | Read/open/download | Upload/edit/delete |
| --- | --- | --- |
| Active vehicle | Allowed | Allowed |
| Archived vehicle | Allowed | `409 conflict`; restore first |
| Draft intervention on active vehicle | Allowed | Allowed |
| Completed or cancelled intervention | Allowed | `409 conflict`; history remains locked |
| Intervention on archived vehicle | Allowed | `409 conflict`; restore vehicle first |
| Active technical note | Allowed | Allowed |
| Archived technical note | Allowed | `409 conflict`; restore first |

## Supported byte signatures

The detector uses uploaded bytes, never the extension or multipart media type. It accepts only:

| Stored media type | Required evidence | `/content` disposition |
| --- | --- | --- |
| `application/pdf` | `%PDF-` signature | Inline |
| `image/jpeg` | JPEG start-of-image marker | Inline |
| `image/png` | Complete PNG signature | Inline |
| `image/webp` | RIFF container with `WEBP` form type | Inline |
| `image/heic` | ISO-BMFF `ftyp` with an approved HEIC brand | Attachment |
| `image/heif` | ISO-BMFF `ftyp` with an approved HEIF brand | Attachment |

Empty/truncated input, unsupported data, spoofed headers/extensions, malformed RIFF/ISO-BMFF
containers, ambiguous generic brands, and AVIF are rejected. The positive and negative fixtures
in `src/models/attachment.rs` are the executable detector contract.

## Application routes

The JSON routes are:

| Method and route | Purpose |
| --- | --- |
| `GET/POST /api/v1/vehicles/{id}/attachments` | List or upload for one vehicle. |
| `GET/POST /api/v1/interventions/{id}/attachments` | List or upload for one intervention. |
| `GET/POST /api/v1/technical-notes/{id}/attachments` | List or upload for one note. |
| `GET/PATCH/DELETE /api/v1/attachments/{id}` | Read, edit display details, or delete. |
| `GET /api/v1/attachments/{id}/content` | Inline-capable authenticated response. |
| `GET /api/v1/attachments/{id}/download` | Forced authenticated download. |

The matching server-rendered workflows live below `/vehicles`, `/interventions`, `/knowledge`,
and `/attachments`. Forms work as ordinary multipart or URL-encoded submissions without
JavaScript; HTMX is an optional enhancement. Crafted owner/attachment combinations return safe
not-found behavior rather than revealing the real owner.

Every content response derives `Content-Type` and `Content-Length` from persisted server-derived
metadata, emits a sanitized ASCII filename fallback plus RFC 5987 UTF-8 filename, and includes
`Cache-Control: private, no-store` and `X-Content-Type-Options: nosniff`. `/download` always uses
`attachment`; `/content` is inline only for PDF, JPEG, PNG, and WebP. Range behavior is not part of
this release.

## Pinned experimental behavior

This contract was checked against the pinned SurrealDB server and Rust SDK 3.2.1. File support is
experimental in this release:

- The server requires `SURREAL_CAPS_ALLOW_EXPERIMENTAL=files`. Embedded test clients opt into only
  `ExperimentalFeature::Files`.
- A persistent local backend uses `BACKEND "file:/absolute/path"`, and that path must be within
  `SURREAL_BUCKET_FOLDER_ALLOWLIST`. Development permits only
  `/home/nonroot/pipauto_attachments` and mounts a dedicated named volume there.
- Disposable tests use
  `DEFINE BUCKET pipauto_attachments BACKEND "memory" PERMISSIONS NONE`. Bucket definitions and
  memory objects are isolated by namespace/database selection.
- The SDK exposes `surrealdb::types::File { bucket, key }`; `File::new` normalizes the key to a
  leading slash. It exposes binary payloads as `surrealdb::types::Bytes`. `Vec<u8>` deliberately
  does not implement `SurrealValue`, so callers must bind `Bytes` explicitly.
- File pointers and byte payloads can be query parameters. The verified form is
  `file::put_if_not_exists($file, $bytes)`, with `$file: File` and `$bytes: Bytes`; no filename or
  object key is interpolated into SurrealQL.
- `file::put`, `file::put_if_not_exists`, `file::get`, `file::head`, `file::exists`,
  `file::delete`, and `file::list` are database functions in 3.2.1. A missing `file::get` or
  `file::head` result is `NONE`; later adapters must translate that explicitly rather than treating
  it as an empty successful object.
- `type::file` is experimental but only constructs a typed pointer; it does not access an object.
  Pipauto uses it as a non-mutating capability check, then reads `INFO FOR DB`, whose `buckets`
  catalog map exposes bucket definitions. Readiness reports only `ready`, `missing`,
  `misconfigured`, or `unavailable`.

References: [bucket definitions](https://surrealdb.com/docs/learn/schema-management/files/buckets),
[file values](https://surrealdb.com/docs/learn/schema-management/files/working-with-files), and
[file functions](https://surrealdb.com/docs/reference/query-language/functions/database-functions/file).

## Request and memory limits

`attachments.maximum_file_bytes` defaults to and cannot exceed 26,214,400 bytes (25 MiB). A
64 KiB multipart allowance produces a fixed global envelope of 26,279,936 bytes. Zero, oversized,
missing, and malformed configuration fails startup with an error that does not echo configured
data.

Raising the global middleware limit does not enlarge existing unsafe routes. Every current unsafe
API or browser route has an explicit route limit, and all retain their former effective 64 KiB
bound or a stricter one. The route audit test requires an explicit layer on every unsafe route.

One multipart request may contain exactly one non-empty `file` field, optional singleton
`display_name` and `caption` UTF-8 text fields, and at most one `_csrf` field. A header CSRF token
may be used instead; if header and form tokens are both supplied they must match. Unknown or
duplicate fields, multiple files, nested multipart, invalid UTF-8 text, malformed boundaries, and
oversized requests fail without persisting an exposed attachment. A failed browser upload retains
only safe text fields and requires the user to reselect the file.

## Explicit reconciliation

The `attachment_reconciliation` Loco task is dry-run-only when invoked without arguments. It scans
all `pending`, `stored`, and `deleting` records plus every bounded bucket page before any apply
operation can start. Its output contains counts and attachment IDs only; filenames, captions,
owners, file pointers, bucket keys, credentials, and bytes are excluded.

Apply is deliberately gated by two operator assertions:

```bash
cargo loco task attachment_reconciliation apply:true quiesced_writes:true
```

Before using apply, stop or otherwise quiesce every attachment upload, metadata edit, and delete
request. Apply finalizes a pending row only after reading the complete object, confirming its size
and detected media type, and calculating its checksum. It removes incomplete pending rows through
the deleting state, resumes deleting rows, and removes objects confirmed to have no attachment
record. Missing, wrong-sized, or checksum-mismatched content for a `stored` record is reported and
the record is retained for investigation; reconciliation never invents replacement content.

The task is safe to retry after interruption. An unavailable record store or bucket stops the run
with a fixed diagnostic message rather than printing backend errors or private storage values.

## Safe errors and operator response

| Symptom | Public behavior | Operator action |
| --- | --- | --- |
| Missing/invalid login | `401 unauthenticated` or login redirect | Sign in; do not retry with a copied cookie. |
| Missing, conflicting, or wrong CSRF | `403 forbidden` | Reload the form/shell and resubmit once. |
| Malformed multipart or unsupported bytes | `400 malformed_request` or `422 validation_failed` | Correct the request and reselect the file. |
| File or envelope exceeds its limit | `413 payload_too_large` | Upload a file of at most 25 MiB. |
| Archived/terminal owner mutation | `409 conflict` | Restore an archive where allowed; never unlock history. |
| Unknown attachment or crafted owner pair | `404 not_found` | Return to the authoritative owner page. |
| Known stored row with missing/corrupt object | Safe `503 unavailable` with correlation ID | Preserve evidence, run dry-run reconciliation, restore paired data if required. |
| Bucket unavailable/misconfigured | Startup/readiness or request reports a fixed safe failure | Check capability, catalog, allowlist, mount, and SurrealDB logs. |

Logs, task output, errors, and test artifacts may contain safe attachment IDs and counts. They must
not contain bytes, filenames, captions, customer data, credentials, cookies, CSRF values, bucket
keys, or file pointers.

## Verification matrix

Automated coverage is split by boundary:

- `./scripts/surrealkit test` verifies the private bucket schema, attachment fields/indexes/events,
  exactly-one-owner rules, state constraints, byte-size limits, and legacy incompatibility.
- `cargo test --all-targets` verifies all supported signatures and rejection cases; uploads and
  content headers; API/browser authentication and CSRF; every owner and lifecycle lock; failure
  injection; corruption behavior; and dry-run/apply reconciliation.
- `npx playwright test` verifies vehicle, intervention, and technical-note workflows in desktop,
  tablet, phone, and no-JavaScript modes with Axe checks.
- `./scripts/ci-check` runs formatting, checking, strict Clippy, SurrealKit suites and rollout lint,
  migration tests, Rust tests, and route/task inventories against disposable data.

Persistent disk behavior is an explicit Compose rehearsal because ordinary Rust and browser tests
intentionally use memory buckets. On synthetic data only:

1. Start Compose, apply the complete schema, confirm the bucket catalog, and upload one fixture
   through Pipauto. Save the download SHA-256 and attachment ID.
2. Run `docker-compose stop surrealdb` followed by `docker-compose start surrealdb`; wait for
   readiness, download again, and compare the checksum and metadata.
3. Exercise the [paired backup and isolated restore
   rehearsal](migrations.md#paired-database-and-attachment-backup), then access the restored file
   through a non-public application instance.
4. Record command exit statuses, fixture IDs (not filenames/customer data), checksums, source
   commit, volume names, restored database name, and cleanup outcome in the deployment record.

The existing-database rollout rehearsal uses
`20260721104500__stored_attachment_schema`: load only the reviewed metadata-only fixture, start the
additive phase, capture the reported legacy count, require the deployment gate to permit only
`ready_to_complete`, complete the contract phase, confirm exactly that count was removed, and
require `sync --dry-run` to report `schema already in sync`. Never run this rehearsal against a
shared database. Exact phased commands and backup prerequisites are in the
[migration runbook](migrations.md#deployment-gate-and-phased-rollout).

## Troubleshooting

- If the bucket is missing after `sync`, confirm the selected namespace/database and inspect
  `INFO FOR DB`; startup deliberately does not repair schema.
- If the disk backend is rejected, confirm Compose enabled `files`, the catalog path is exactly
  `/home/nonroot/pipauto_attachments`, and the same path is allowlisted and mounted.
- If the bucket catalog is ready but uploads return `503`, confirm the attachment volume is owned
  by UID/GID `65532`. Stop SurrealDB, run `docker-compose run --rm attachment-volume-init`, restart
  SurrealDB, and run dry-run reconciliation; never delete the volume as an ownership repair.
- If records survive but downloads fail after a restart, inspect both Compose volumes. Restoring
  only the logical export cannot recover attachment bytes.
- If apply reconciliation refuses to run, stop attachment writes and pass both
  `apply:true quiesced_writes:true`; do not bypass the assertion.
- If a stored record is reported missing or mismatched, preserve it and investigate/restore the
  paired backup. Reconciliation intentionally does not delete or fabricate it.
- If an upload returns a size or malformed-body failure before domain validation, check the global
  25 MiB + 64 KiB envelope and the per-route limit before changing configuration.

## SurrealDB upgrade gate

SurrealDB server and Rust SDK 3.2.1 are an experimental, version-specific dependency. Before
changing either version, use an isolated branch and disposable disk volume to revalidate all of
the following; a changelog review alone is insufficient:

1. `files` capability names/flags, the bucket folder allowlist, `DEFINE BUCKET` disk and memory
   syntax, `PERMISSIONS NONE`, and `INFO FOR DB` catalog shape.
2. `File::new` key normalization, `Bytes` parameter binding, serialization of persisted
   `file<pipauto_attachments>` values, and rejection of pointers from other buckets.
3. `file::put_if_not_exists`, `get`, `head`, `exists`, `delete`, and paginated `list` return/error
   semantics, including missing objects, collisions, binary round trips, and bounded listing.
4. The non-atomic upload/delete failure-injection matrix and dry-run/apply reconciliation outcomes.
5. Clean sync plus the complete metadata-only phased rollout, count, deployment gate, contract
   deletion, final catalog, and rollout lint/checksum behavior.
6. Disk persistence across stop/start and container recreation, then paired database-plus-volume
   backup and isolated restore with matching application downloads and SHA-256 values.
7. The full final gate: `cargo fmt --check`, `cargo check --all-targets`,
   `cargo test --all-targets`, `./scripts/surrealkit test`, `npx playwright test`, and
   `./scripts/ci-check`.

Record the tested server image digest, SDK version, application commit, host/container platform,
commands, results, and any changed contract before updating the pins. Do not deploy the upgrade or
edit this guide to claim compatibility until every item passes.
