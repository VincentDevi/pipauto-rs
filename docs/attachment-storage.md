# Attachment storage runtime contract

Pipauto stores attachment bytes in the single private SurrealDB bucket
`pipauto_attachments`. The bucket is schema: application startup and health checks never define,
replace, or remove it and never write probe objects. Tests may explicitly call
`define_attachment_memory_bucket` after selecting a disposable database.

The bucket permission is `NONE`. Browser and API users never operate on bucket values directly;
all object operations go through Pipauto's server-side, root-authenticated SurrealDB adapter.
Bucket names, keys, backend paths, and raw catalog errors are not readiness response data.

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
