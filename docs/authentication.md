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

Load `.env`, start SurrealDB, and explicitly apply the idempotent schema. Normal server startup does
not mutate production or development schemas:

```bash
set -a && source .env && set +a
docker-compose up -d --wait surrealdb
cargo loco task apply_auth_schema
```

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
| `GET /login` | Guest | Complete sign-in page; authenticated users return to `/`. |
| `POST /login` | Guest + login CSRF | Generic invalid-credentials response; success creates a 12-hour session. |
| `GET /` | Authenticated | Workshop shell; guests are redirected to login with a safe local `next`. |
| `GET /setup/status` | Authenticated | HTMX database-status fragment. |
| `POST /logout` | Authenticated + session CSRF | Idempotently revokes the registry session and clears the cookie. |
| `/static/*` | Public | Committed same-origin CSS, JavaScript, and vendored HTMX. |
| `/_health/*` | Public | Non-sensitive machine health only. |

Normal browser redirects use `303`; HTMX requests use `HX-Redirect`. Login forms use a short-lived
10-minute nonce cookie and signed token. Authenticated CSRF tokens are HMAC-signed and bound to the
session `jti`, canonical origin, action, and expiry. Unsafe requests accept one `_csrf` field or one
`X-CSRF-Token`; if both exist they must match. The same-origin JavaScript adds the header to HTMX
unsafe requests, while forms remain fully usable without JavaScript.

Authentication responses use `Cache-Control: no-store`, a restrictive same-origin CSP, no-referrer,
anti-framing, MIME-sniffing protection, and disabled camera, microphone, and geolocation policies.

## Production deployment

Use an HTTPS canonical origin and terminate TLS either in Pipauto's deployment boundary or at a
reverse proxy that forwards only to a protected backend connection. Do not rewrite cookie names or
strip `Set-Cookie`, `Origin`, CSP, or other security headers. Pipauto currently keys throttling from
the direct socket IP and deliberately ignores forwarding headers. Trusted proxy address resolution
must be an explicit later configuration change; do not enable arbitrary `X-Forwarded-For` trust.
Keep system clocks synchronized because JWT and CSRF expiry checks are time based.

## Verification

From a clean checkout, perform setup above, create a user, start `cargo loco start`, and verify wrong
credentials, normal and JavaScript-disabled login/logout, cookie flags, protected-route redirects,
and replay rejection after logout. Stop SurrealDB temporarily and confirm authentication returns a
safe unavailable response instead of granting access. Inspect logs for secrets and credentials.

Run the automated gate:

```bash
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
- **User creation fails:** apply the schema, confirm SurrealDB settings/health, use an interactive terminal, satisfy password boundaries, and check for an existing normalized email.
- **Invalid credentials:** email, password, inactive users, and unknown users intentionally share one generic response.
- **Too many attempts:** wait for the 15-minute temporary block. The limit is five failures in 15 minutes per normalized-email and direct-socket-IP digest.
- **Revoked or expired session:** sign in again; a valid JWT cannot override registry revocation or fixed expiry.
- **CSRF failure:** reload the page, submit from the canonical origin, and ensure a proxy preserves `Origin`, cookies, and the single matching token.
- **SurrealDB unavailable:** check `/_health/surrealdb`, Compose state, endpoint, namespace, database, and credentials. Authentication fails closed with `503`.
- **Missing template/static response:** run from the repository root and confirm `assets/views` and `assets/static` are present in the deployment artifact.
- **Bad proxy headers:** this milestone ignores forwarding headers. Correct the direct backend topology; do not trust client-supplied forwarding headers.

Framework references: [Loco authentication](https://loco.rs/docs/the-app/authentication/),
[Loco tasks](https://loco.rs/docs/the-app/tasks/), and
[Loco middleware](https://loco.rs/docs/the-app/middleware/).
