# Authentication operations

Pipauto uses Loco 0.16.4's Argon2id and JWT APIs behind application-owned adapters. A JWT is only
the signed browser credential: every request also looks up its SHA-256 `jti` digest in the strict
SurrealDB session registry and verifies that the associated user is active. The exact JWT expiry is
copied into the registry before the token is returned. Request activity never extends it.

## Configuration

Set these values before running a non-test command:

| Variable | Meaning |
| --- | --- |
| `PIPAUTO_JWT_SECRET` | Base64 for at least 32 random bytes used by Loco JWT signing. |
| `PIPAUTO_CSRF_SECRET` | Different Base64 value for at least 32 random bytes used by HMAC CSRF and throttle digests. |
| `PIPAUTO_CANONICAL_ORIGIN` | Exact origin, such as `http://localhost:5150` in development or `https://pipauto.example` in production. No path, query, fragment, or credentials. |
| `PIPAUTO_SESSION_LIFETIME_SECONDS` | Optional; defaults to and is committed as `43200` (12 hours). Accepted range is 300–86400. |

Generate the secrets independently and copy their output into `.env`; do not reuse either value:

```bash
openssl rand -base64 32
openssl rand -base64 32
```

Configuration errors redact secret values. Secrets stay outside Loco's debug-printable raw auth
configuration.

Development uses `pipauto_session` and `pipauto_login_csrf`. Production uses
`__Host-pipauto_session` and `__Host-pipauto_login_csrf`. All are `HttpOnly`, `SameSite=Lax`,
`Path=/`, have no `Domain`, and use fixed `Max-Age`; production cookies additionally use `Secure`.
There are no refresh tokens, sliding expiry, remember-me sessions, or browser-side token storage.

## Setup and routine tasks

Load `.env`, start SurrealDB, install the pinned SurrealKit CLI, and sync a new development
database. Normal server startup does not mutate production or development schemas:

```bash
set -a && source .env && set +a
docker-compose up -d --wait surrealdb
cargo install surrealkit --version 0.7.0 --locked
surrealkit --version
./scripts/surrealkit sync
```

Pipauto pins SurrealKit `0.7.0` with SurrealDB server and Rust SDK `3.2.1`. SurrealKit is developer
and CI tooling, not an application dependency. `scripts/surrealkit` maps the existing
`SURREALDB_*` settings to SurrealKit's HTTP connection variables in process memory; it never puts
credentials in `surrealkit.toml` or command arguments.

For an existing database, do not run `sync`: it can prune definitions. Run the read-first adoption
gate instead:

```bash
./scripts/surrealkit baseline-authentication
```

The gate queries `INFO FOR DB` and each authentication table, compares the normalized live catalog
with `database/tests/fixtures/authentication_catalog.json`, and stops on a missing, extra, or changed
definition before SurrealKit writes metadata. Only a complete match may invoke `rollout baseline`.
It fingerprints all logical fields of every user, session, and throttle record before and after the
baseline, and fails if row contents, record IDs, timestamps, hashes, active states, expiries, or
throttle state change. Output names only the failed phase/table and never prints catalog exports,
credentials, password hashes, sessions, or tokens.

Provision users interactively. The two password prompts do not echo; never put a password in a
shell argument or environment variable:

```bash
cargo loco task create_user email:filippo@example.com display_name:Filippo
```

Passwords must contain at least 12 Unicode scalar values, use at most 1,024 UTF-8 bytes, contain
only printable Unicode (spaces are allowed), and must not equal the normalized email. Display names
are non-empty and at most 120 Unicode scalar values. Duplicate normalized email addresses are
rejected. The task fails rather than reading from redirected input when no attached terminal is
available.

Remove expired registry entries as routine maintenance:

```bash
cargo loco task purge_expired_auth_sessions
```

To contain a compromised account, obtain the record id printed by `create_user`, open the SurrealDB
SQL shell, and run both statements in the `pipauto` namespace and selected database:

```sql
UPDATE user:THE_RECORD_ID SET active = false, updated_at = time::now();
UPDATE auth_session SET revoked_at = time::now()
WHERE user = user:THE_RECORD_ID AND revoked_at = NONE;
```

The first statement prevents every login and request; the second immediately revokes existing
sessions. Restore `active = true` only after the account is safe.

## Browser behavior

| Route | Access | Behavior |
| --- | --- | --- |
| `GET /login` | Guest-only | Complete sign-in page; authenticated users return to `/`. |
| `POST /login` | Guest-only + login CSRF | Generic invalid-credentials response; success creates a 12-hour session. |
| `GET /` | Authenticated | Workshop shell; guests are redirected to login with a safe local `next`. |
| `GET /setup/status` | Authenticated | HTMX database-status fragment. |
| `POST /logout` | Authenticated + session CSRF | Idempotently revokes the registry session and clears the cookie. |
| `/static/*` | Public | Committed same-origin CSS, JavaScript, and vendored HTMX. |
| `GET /_health` | Public | Loco liveness response with no application data. |
| `GET /_health/surrealdb` | Public | Non-sensitive database availability state only. |
| `GET /_ping` | Public | Loco process ping response with no application data. |
| `GET /_readiness` | Public | Loco readiness response with no application data. |

Normal browser redirects use `303`; HTMX requests use `HX-Redirect`. Login forms use a short-lived
10-minute nonce cookie and signed token. Authenticated CSRF tokens are HMAC-signed and bound to the
session `jti`, canonical origin, action, and expiry. Unsafe requests accept one `_csrf` field or one
`X-CSRF-Token`; if both exist they must match. The same-origin JavaScript adds the header to HTMX
unsafe requests, while forms remain fully usable without JavaScript.

Authentication routes and authenticated application routes apply `Cache-Control: no-store` in a
route layer, so handler, extractor, body-limit, and media-type errors inherit the same policy.
Responses use a restrictive same-origin CSP, no-referrer,
anti-framing, MIME-sniffing protection, and disabled camera, microphone, and geolocation policies.

## Production deployment

Use an HTTPS canonical origin and terminate TLS either in Pipauto's deployment boundary or at a
reverse proxy that forwards only to a protected backend connection. Do not rewrite cookie names or
strip `Set-Cookie`, `Origin`, CSP, or other security headers. Pipauto currently keys throttling from
the direct socket IP and deliberately ignores forwarding headers. Trusted proxy address resolution
must be an explicit later configuration change; do not enable arbitrary `X-Forwarded-For` trust.
Keep system clocks synchronized because JWT and CSRF expiry checks are time based.

## Clean-checkout end-to-end verification

Run this procedure from a fresh checkout with no existing `.env` or SurrealDB volume. Credential
entry is deliberately performed in the browser or non-echoing terminal prompt, never in a shell
argument or environment variable.

1. Copy `.env.example`, generate the two independent secrets with the commands under
   [Configuration](#configuration), fill in `.env`, then load it:

   ```bash
   cp .env.example .env
   set -a && source .env && set +a
   ```

2. Start and health-check SurrealDB, install the pinned CLI, then sync the clean database:

   ```bash
   docker-compose up -d --wait surrealdb
   docker-compose exec surrealdb /surreal isready --endpoint http://localhost:8000
   cargo install surrealkit --version 0.7.0 --locked
   ./scripts/surrealkit sync
   ```

3. Create the first user. Enter and confirm the password only at the two non-echoing prompts:

   ```bash
   cargo loco task create_user email:filippo@example.com display_name:Filippo
   ```

4. Start Pipauto and leave its terminal visible for log inspection:

   ```bash
   cargo loco start
   ```

5. In a private browser window, open <http://localhost:5150/> and confirm navigation ends at
   `/login?next=/`. Submit an incorrect password and confirm only `Invalid credentials.` appears;
   the submitted password must not appear in the page, response, application output, or logs.
6. Sign in with the provisioned account. Confirm `/` renders the workshop shell. In browser
   developer tools, inspect `pipauto_session`: it must have `HttpOnly`, `SameSite=Lax`, `Path=/`,
   no `Domain`, a 43,200-second `Max-Age`, and no `Secure` attribute in development. Copy the cookie
   value temporarily inside developer tools for the replay check; do not paste it into a terminal
   or log.
7. Sign out. Confirm navigation reaches `/login`, the cookie is deleted with the same attributes,
   and restoring the copied old value in developer tools still cannot open `/`.
8. Disable JavaScript for `localhost` (which disables HTMX), reload `/login`, and repeat a normal
   login and logout. Confirm the standard forms have the same result, then re-enable JavaScript.
9. Sign in once more, stop SurrealDB from a second terminal, and request `/` again:

   ```bash
   docker-compose stop surrealdb
   ```

   Confirm Pipauto returns the safe authentication-unavailable response with HTTP `503`; it must
   not render the protected shell or expose database details. Restore the database afterward:

   ```bash
   docker-compose start surrealdb
   docker-compose up -d --wait surrealdb
   ```

10. Review the complete Pipauto output from steps 3–9. It may contain safe correlation identifiers
    and throttle timing, but must contain no configured secret, password, JWT, CSRF token, session
    identifier, submitted email, or database password.

For a production deployment, repeat the browser checks against the HTTPS origin. Cookie names must
be `__Host-pipauto_session` and `__Host-pipauto_login_csrf`, and both cookies must additionally have
`Secure`.

## Automated verification

Run the automated gate:

```bash
cargo install surrealkit --version 0.7.0 --locked
surrealkit --version
./scripts/surrealkit test --suite 'authentication*'
cargo fmt --check
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo loco routes
cargo loco task
```

## Troubleshooting

- **Missing, short, equal, or invalid-base64 secrets:** generate two new independent values with the command above and reload `.env`.
- **Production refuses an HTTP origin:** configure the externally visible `https://` origin. HTTP is development-only.
- **Cookie name mismatch or immediate sign-out:** do not manually rename cookies; ensure production reaches the app over HTTPS and the client accepts `__Host-` cookies.
- **Tokens expire too early or CSRF suddenly fails:** synchronize the application host's system clock and verify the configured 12-hour lifetime.
- **SurrealKit is missing or rejected:** install exactly `0.7.0`; the wrapper fails before any
  schema operation when the CLI is absent or has a different version.
- **Authentication baseline reports catalog drift:** do not retry with `sync` and do not edit the
  snapshots. Compare the named table with the committed schema and reconcile the database through
  a reviewed rollout before attempting the baseline again.
- **User creation fails:** sync a clean database (or complete the existing-database baseline), confirm SurrealDB settings/health, use an interactive terminal, satisfy password boundaries, and check for an existing normalized email.
- **Invalid credentials:** email, password, inactive users, and unknown users intentionally share one generic response.
- **Too many attempts:** wait for the 15-minute temporary block. The limit is five failures in 15 minutes per normalized-email and direct-socket-IP digest.
- **Revoked or expired session:** sign in again; a valid JWT cannot override registry revocation or fixed expiry.
- **CSRF failure:** reload the page, submit from the canonical origin, and ensure a proxy preserves `Origin`, cookies, and the single matching token.
- **SurrealDB unavailable:** check `/_health/surrealdb`, Compose state, endpoint, namespace, database, and credentials. Authentication fails closed with `503`.
- **Missing template/static response:** run from the repository root and confirm `assets/views` and `assets/static` are present in the deployment artifact.
- **Bad proxy headers:** this milestone ignores forwarding headers. Correct the direct backend topology; do not trust client-supplied forwarding headers.

Resolved framework version: `loco-rs` 0.16.4. The application adapters use the versioned
[`JWT` API](https://docs.rs/loco-rs/0.16.4/loco_rs/auth/jwt/struct.JWT.html),
[`hash_password` API](https://docs.rs/loco-rs/0.16.4/loco_rs/hash/fn.hash_password.html),
[`verify_password` API](https://docs.rs/loco-rs/0.16.4/loco_rs/hash/fn.verify_password.html), and
[`Task` API](https://docs.rs/loco-rs/0.16.4/loco_rs/task/trait.Task.html). Framework guides:
[authentication](https://loco.rs/docs/the-app/authentication/),
[task variables](https://loco.rs/docs/the-app/tasks/), and
[middleware](https://loco.rs/docs/the-app/middleware/).
