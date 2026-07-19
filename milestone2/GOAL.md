# Pipauto — User Authentication Milestone

This document is the source of truth for the Linear issues required to complete the second Pipauto milestone, **Handle user auth**.

## Milestone outcome

At the end of this milestone, Pipauto has a secure, server-rendered authentication boundary that:

- Allows a provisioned Pipauto user to sign in with an email address and password.
- Uses Loco's JWT generation, validation, cookie extraction, routing, task, and Axum integration facilities.
- Persists application users and revocable browser sessions in SurrealDB rather than using Loco's generated SeaORM user model.
- Authenticates each private request by validating both a signed JWT and its active SurrealDB session record.
- Protects all state-changing browser requests against CSRF.
- Works with normal HTML forms and HTMX progressive enhancement.
- Prevents unauthenticated access to every private server route, independently of frontend behavior.
- Provides a documented, terminal-only process for creating the first user.

Public registration, password recovery, email verification, social login, multi-factor authentication, API keys, persistent “remember me” sessions, roles, granular permissions, and a session-management UI are outside this milestone.

## Linear metadata

Apply the following metadata to every issue created from this document:

| Field | Value |
| --- | --- |
| Team | `VincentDevi-Perso` |
| Project | `Pipauto` |
| Milestone | `Handle user auth` |
| Assignee | Unassigned |
| Cycle | None |
| Due date | None |

Create the issues in the order below and preserve the dependency relationships stated in each issue.

## Investigated authentication design

### Why Loco's generated authentication cannot be copied unchanged

Loco's SaaS authentication starter is designed around API routes, bearer or cookie JWTs, and its generated SeaORM user model. Pipauto was deliberately created from the Lightweight Service starter and owns a separate SurrealDB persistence layer. Adding the generated SaaS model would introduce a second database abstraction, violate the Project Setup architecture, and make authentication persistence inconsistent with the rest of the application.

Loco 0.16.x exposes database-independent JWT creation and validation and can extract JWTs from cookies. Pipauto will reuse those framework facilities while implementing its own SurrealDB user and session repositories. The exact resolved Loco version and public API signatures must be recorded before implementation. If the project resolves a version earlier than the database-independent JWT API, upgrade through `cargo add loco-rs@<compatible-version>` rather than editing `Cargo.toml` manually.

References:

- [Loco authentication](https://loco.rs/docs/extras/authentication/)
- [Loco controller authentication and cookie JWT configuration](https://loco.rs/docs/the-app/controller/)
- [Loco JWT API](https://docs.rs/loco-rs/latest/loco_rs/auth/jwt/struct.JWT.html)
- [Loco task system](https://loco.rs/docs/getting-started/guide/#tasks-export-data-report)

### Why SurrealDB record authentication is not the browser session

`AppDatabase` owns one application-managed SurrealDB client shared across requests. SurrealDB authentication state is associated with the database connection. Signing that shared client in as an application user for a browser request would mix identities across concurrent requests and break the shared-service design.

Pipauto therefore connects to SurrealDB using its server credentials and treats `user` and `auth_session` as application-owned records. Authorization is enforced by the Loco/Axum request boundary and application services. SurrealDB record authentication may be reconsidered only if a later architecture gives every request an isolated database session.

Reference: [SurrealDB users and sessions](https://surrealdb.com/docs/learn/security/authentication/authentication).

### Selected browser-session design

1. A successful login creates a random 256-bit session identifier, called `jti`.
2. Pipauto stores only `SHA-256(jti)` in `auth_session` and never stores the raw identifier or complete JWT.
3. Loco signs a JWT containing the user's stable record identifier in `pid` and the random identifier in the custom `jti` claim.
4. The JWT is sent only in an `HttpOnly` session cookie.
5. An authenticated request must pass JWT signature and expiry validation, locate a matching unrevoked and unexpired `auth_session`, and load an active user whose identifier matches both records.
6. Logout revokes the session record and expires the browser cookie. A copied JWT stops working as soon as its registry record is revoked.
7. Sessions have a fixed 12-hour lifetime. This milestone has no refresh token, sliding expiry, or persistent-login option.

This hybrid retains Loco's JWT integration while providing immediate server-side revocation.

### Security baseline

- Passwords are encoded as Argon2id PHC strings using Loco's password-hashing functions. Application code must not implement password cryptography.
- JWT signing uses one environment-provided secret containing at least 32 random bytes. Example or committed configuration must not contain a production-capable secret.
- Development over HTTP uses `pipauto_session`; production over HTTPS uses `__Host-pipauto_session`.
- The production cookie is `Secure`, `HttpOnly`, `SameSite=Lax`, `Path=/`, and has no `Domain` attribute.
- Authentication and login pages use `Cache-Control: no-store`.
- Unsafe methods are `POST`, `PUT`, `PATCH`, and `DELETE`; all must pass CSRF validation.
- Normal forms submit `_csrf`; HTMX submits the same value through `X-CSRF-Token`.
- Login uses a signed pre-authentication CSRF token. Successful login discards the pre-authentication state and creates a new authenticated session.
- Credential errors are deliberately generic. Logs may contain an event category and request correlation ID, but never passwords, JWTs, raw session identifiers, CSRF tokens, password hashes, or JWT secrets.
- Login throttling is temporary and account-aware. It must not permanently lock a user out or reveal whether an account exists.

Security references:

- [OWASP Authentication Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html)
- [OWASP Session Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html)
- [OWASP CSRF Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html)

---

## Issue 1 — Define authentication configuration and security boundaries

**Priority:** High
**Dependencies:** Project Setup milestone

### Objective

Define the authentication contract, environment-backed settings, dependency boundary, and safe startup behavior before implementing persistence or routes.

### Dependency-management rule

Do not edit `Cargo.toml` manually. Inspect the resolved framework and dependencies first:

```bash
cargo tree -p loco-rs
cargo metadata --no-deps
```

Add or adjust direct dependencies only with `cargo add` or `cargo remove`. The implementation is expected to need Loco's JWT and password-hashing support plus audited crates for secure random generation, SHA-256 digesting, constant-time comparison, terminal password input, and secret handling. Record why every new direct dependency is needed.

Do not add a second web framework, authentication service, SeaORM model, Redis session store, or JavaScript package manager.

### Public `AuthSettings` type

Add a typed, validated `AuthSettings` value to the application's environment-backed configuration. It must contain:

- JWT signing secret, held in a secrecy-preserving wrapper whose debug output is redacted.
- Fixed session lifetime, defaulting to `12h` and limited to a documented safe range.
- Development and production cookie names.
- Cookie secure-mode policy derived from the application environment.
- Cookie `SameSite` policy fixed to `Lax` for this milestone.
- Login-attempt window, maximum attempts, and temporary block duration.
- CSRF signing secret, distinct from the JWT secret and held in a redacted wrapper.
- Canonical externally visible origin used for exact `Origin` and `Referer` validation.

Validation must reject:

- Missing secrets.
- Secrets shorter than 32 bytes or equal to committed example values.
- A zero or out-of-range session lifetime.
- An invalid canonical origin, an origin containing a path/query/fragment, or production HTTP.
- A production cookie that is not `Secure`, does not use the `__Host-` prefix, has a `Domain`, or does not use `Path=/`.
- An empty cookie name or one containing invalid cookie characters.
- Zero throttling limits or durations.
- Reusing the JWT secret as the CSRF secret.

### Loco integration requirements

- Configure Loco's JWT location as a cookie, not a bearer header or query parameter.
- Do not accept JWTs in URLs under any environment.
- Pin behavior to the resolved Loco 0.16.x API and document the confirmed version.
- Fail application startup before binding the HTTP listener when authentication configuration is invalid.
- Keep raw configuration parsing in the configuration layer; controllers receive validated settings or an authentication service.
- Add `docs/authentication.md` with the investigated design, trust boundaries, request flow, rejected alternatives, and links to the official references above.

### Secret-handling requirements

- Add safe placeholders and variable names to `.env.example` without usable secrets.
- Ensure real `.env` files remain ignored.
- Provide a documented command that generates at least 32 random bytes for each secret without committing the output.
- Redact secrets from configuration errors, `Debug` output, tracing spans, panic reports, and test snapshots.
- Never display a secret in the README's example output.

### Acceptance criteria

- [ ] The resolved Loco version and required JWT APIs are documented.
- [ ] `AuthSettings` parses and validates every required setting.
- [ ] Production refuses HTTP origins and insecure cookie configuration.
- [ ] JWT and CSRF secrets are distinct, at least 32 bytes, environment-provided, and redacted.
- [ ] Loco is configured to read JWTs only from the selected session cookie.
- [ ] No SeaORM authentication model or second persistence stack is introduced.
- [ ] `docs/authentication.md` explains the JWT-plus-registry choice and why SurrealDB record authentication is not used.
- [ ] No secret appears in logs, errors, fixtures, snapshots, or version-controlled examples.

### Verification

```bash
cargo metadata --no-deps
cargo check
cargo test auth_settings
cargo loco routes
```

---

## Issue 2 — Create the SurrealDB user and authentication-session persistence model

**Priority:** High
**Dependencies:** Issue 1

### Objective

Create explicit SurrealDB schemas and repository contracts for application users, revocable JWT sessions, and login throttling without leaking persistence behavior into controllers or domain models.

### `user` schema

Define a strict `user` table with:

| Field | Requirement |
| --- | --- |
| `id` | SurrealDB record identifier; generated by the database and exposed as the stable application-user identifier. |
| `email` | Original, trimmed address used for display; never used directly for equality lookup. |
| `email_normalized` | Trimmed and ASCII-lowercased lookup value with a unique index. Unicode local-part rewriting is outside scope. |
| `display_name` | Trimmed non-empty user-facing name with a documented maximum length. |
| `password_hash` | Argon2id PHC string; prohibited from normal user projections and debug output. |
| `active` | Boolean controlling whether the user may authenticate; defaults to true. |
| `created_at` | Database-generated UTC timestamp. |
| `updated_at` | UTC timestamp changed on persisted updates. |

The repository must map database records into a domain `User` that does not expose `password_hash`. Credential lookup returns a separate internal `UserCredentials` value whose debug output redacts the hash.

### `auth_session` schema

Define a strict `auth_session` table with:

| Field | Requirement |
| --- | --- |
| `id` | Database-generated record identifier used only internally. |
| `user` | Record reference to `user`. |
| `jti_digest` | Lowercase SHA-256 digest of the random JWT `jti`; unique and never the raw value. |
| `issued_at` | UTC issuance time. |
| `expires_at` | UTC fixed expiry, exactly matching the JWT expiration. |
| `revoked_at` | Optional UTC revocation time. |
| `last_seen_at` | Optional coarse audit timestamp; it must not extend expiry and should not be written on every request. |
| `created_ip_digest` | Optional keyed digest or omitted value; never persist a raw client IP merely for convenience. |
| `user_agent_summary` | Optional length-limited summary with control characters removed; never use it for authorization. |

Add indexes supporting unique `jti_digest`, lookup by `user`, and cleanup by `expires_at`. Expired and revoked sessions remain invalid even before cleanup runs.

### Login-throttle schema

Store throttling state in a separate strict table keyed by a digest of the normalized login identifier and a conservative client-network key. It must contain only counters and time boundaries needed to enforce a temporary limit. Do not store the submitted email or raw IP in this table.

Use a documented default of five failed attempts within fifteen minutes followed by a fifteen-minute block. Successful authentication clears the relevant failure state. Unknown and known accounts must follow the same externally visible throttling path.

### Repository contracts

Define technology-independent traits:

```rust
trait UserRepository {
    async fn find_by_id(&self, id: &UserId) -> Result<Option<User>, RepositoryError>;
    async fn find_credentials_by_email(
        &self,
        normalized_email: &NormalizedEmail,
    ) -> Result<Option<UserCredentials>, RepositoryError>;
    async fn create(&self, new_user: NewUserRecord) -> Result<User, RepositoryError>;
}

trait AuthSessionRepository {
    async fn create(&self, session: NewAuthSession) -> Result<AuthSession, RepositoryError>;
    async fn find_active(
        &self,
        jti_digest: &SessionDigest,
        now: DateTime<Utc>,
    ) -> Result<Option<AuthSession>, RepositoryError>;
    async fn revoke(
        &self,
        jti_digest: &SessionDigest,
        now: DateTime<Utc>,
    ) -> Result<RevokeOutcome, RepositoryError>;
    async fn revoke_all_for_user(
        &self,
        user_id: &UserId,
        now: DateTime<Utc>,
    ) -> Result<u64, RepositoryError>;
    async fn delete_expired(&self, now: DateTime<Utc>) -> Result<u64, RepositoryError>;
}
```

Add a dedicated throttling contract rather than embedding rate-limit queries in the authentication service.

### Persistence requirements

- Place contracts in `repositories` and SurrealDB implementations in `repositories/surreal`.
- Use bound parameters for every dynamic SurrealQL value.
- Treat duplicate email creation as a typed conflict without returning raw database errors.
- Make session creation fail if the digest already exists; never overwrite an existing session.
- Make revocation idempotent so concurrent logout requests are safe.
- Do not convert connectivity, timeout, or query failures into “user not found” or “session not found.”
- Provide an explicit schema application mechanism compatible with the Project Setup persistence strategy and test it against a fresh in-memory database.
- Define cleanup behavior, but do not add an always-running scheduler unless the existing application already has one. A registered maintenance task is sufficient for this milestone.

### Acceptance criteria

- [ ] Strict `user`, `auth_session`, and throttle schemas exist with the required fields and indexes.
- [ ] Normalized email uniqueness is enforced by SurrealDB, not only application code.
- [ ] Raw session identifiers and JWTs are never persisted.
- [ ] Normal user values cannot expose the password hash.
- [ ] Repository traits are independent of SurrealDB types.
- [ ] SurrealDB adapters use bound parameters and typed errors.
- [ ] Concurrent or repeated revocation is idempotent.
- [ ] Expired and revoked sessions cannot be returned as active.
- [ ] A fresh in-memory database can apply the authentication schema and run repository tests.

### Verification

```bash
cargo check
cargo test user_repository
cargo test auth_session_repository
cargo test login_throttle_repository
```

---

## Issue 3 — Implement password authentication and the first-user administration task

**Priority:** High
**Dependencies:** Issues 1 and 2

### Objective

Implement safe password handling and a terminal-only, repeatable process for creating Pipauto users without exposing a registration endpoint.

### Password rules

- Trim email and display-name input, but never trim or otherwise rewrite passwords.
- Normalize lookup email using the exact rule defined in Issue 2.
- Require a minimum password length of 12 Unicode scalar values and a maximum UTF-8 byte length of 1,024 to bound hashing work.
- Allow spaces and all printable Unicode; do not require arbitrary mixtures of character classes.
- Reject known-invalid input such as an empty password or a password equal to the normalized email.
- Encode passwords through Loco's Argon2id password-hashing function and store the returned PHC string unchanged.
- Verify through Loco's verification function so PHC parameters remain self-describing.
- Run one dummy Argon2id verification when an email is unknown so the request does not return substantially faster than a wrong password for a known user.
- Return one public `Invalid credentials` outcome for unknown user, wrong password, and inactive user.
- Never log password length, content, hash, verification details, or whether an address exists.

### `create_user` Loco task

Register a Loco task named `create_user` and list it through `cargo loco task`.

The task must:

1. Boot the normal application context and retrieve the shared `AppDatabase` and user repository.
2. Accept email and display name as non-secret Loco task variables using the argument syntax supported by the resolved CLI version.
3. Read the password twice from an attached terminal without echoing.
4. Fail when no interactive terminal is available instead of falling back to a visible command argument.
5. Validate both entries and require an exact match.
6. Hash the password only after all input validation succeeds.
7. Create the user through the same application service used by future administration features.
8. Print only the new stable user identifier, normalized email, and success status.
9. Return a non-zero exit status for duplicate email, invalid input, database failure, or password mismatch.
10. Leave an existing record completely unchanged when creation fails or the task is rerun.

Do not accept a password through task variables, environment variables, `.env`, stdin redirected from a file, or shell command arguments.

### Maintenance task

Register a separate `purge_expired_auth_sessions` task that deletes only sessions whose `expires_at` is in the past. It must report the count removed and must not revoke active sessions.

### Tests

- Unit-test email normalization and password boundary values.
- Test valid hash creation and verification without snapshotting hashes.
- Test wrong passwords and dummy verification for unknown users.
- Test inactive users through the same public failure path.
- Test successful task creation against in-memory SurrealDB.
- Test duplicate execution and mismatched password confirmation.
- Abstract the terminal reader so tests never require a real interactive prompt.

### Acceptance criteria

- [ ] Password hashes are Argon2id PHC strings produced and verified by Loco facilities.
- [ ] Unknown users perform dummy password verification.
- [ ] All credential failures return the same public outcome.
- [ ] `cargo loco task` lists `create_user` and `purge_expired_auth_sessions`.
- [ ] The creation task never accepts or prints a password.
- [ ] Duplicate creation is non-destructive and returns failure.
- [ ] No public registration route exists.
- [ ] Expired-session cleanup cannot remove active sessions.

### Verification

```bash
cargo test password_authentication
cargo test create_user_task
cargo test purge_expired_auth_sessions
cargo loco task
```

Manually run the documented `create_user` command against development SurrealDB and confirm that the terminal prompts twice without echo.

---

## Issue 4 — Implement Loco JWT issuance and the revocable authentication service

**Priority:** High
**Dependencies:** Issues 1–3

### Objective

Create the application service that verifies credentials, issues Loco JWTs, registers their server-side sessions, validates authenticated requests, and revokes sessions safely.

### Public service types

#### `AuthService`

A shared, cheap-to-clone service composed from validated `AuthSettings`, the clock, secure random source, Loco JWT operations, `UserRepository`, `AuthSessionRepository`, and throttle repository.

It must expose workflows equivalent to:

```rust
async fn login(&self, command: LoginCommand) -> Result<LoginOutcome, AuthError>;
async fn authenticate(&self, encoded_jwt: &str) -> Result<AuthenticatedUser, AuthError>;
async fn logout(&self, encoded_jwt: Option<&str>) -> Result<LogoutOutcome, AuthError>;
```

Controllers must not call repositories, hash passwords, generate tokens, or validate JWTs directly.

#### `AuthenticatedUser`

A presentation-safe value containing only:

- Stable `UserId`.
- Email.
- Display name.
- Session expiry.

It must not contain the password hash, JWT, raw `jti`, session digest, CSRF signing material, or repository handles.

#### Outcomes and errors

Use explicit outcomes for success, invalid credentials, temporary throttle, and idempotent logout. Infrastructure failures must be typed separately from authentication failures so controllers can return a safe service-unavailable response rather than claiming credentials are wrong.

### Login workflow

Perform these steps in order:

1. Normalize and validate input.
2. Check throttling without revealing whether the user exists.
3. Load credentials or select the fixed dummy PHC hash.
4. Perform Argon2id verification exactly once.
5. Record failure and return the generic invalid-credential outcome for unknown, wrong-password, or inactive users.
6. Clear throttling state after a successful verification.
7. Generate a 256-bit `jti` from the operating system CSPRNG.
8. Compute `SHA-256(jti)` for persistence.
9. Set `issued_at` from the injected clock and `expires_at` exactly 12 hours later.
10. Persist `auth_session` before making a JWT or cookie available to the controller.
11. Generate the JWT through Loco with `pid` equal to the stable user identifier and custom `jti` equal to the raw random identifier.
12. If JWT generation fails after persistence, revoke the newly created session before returning the error.
13. Return the encoded JWT, cookie expiry data, authenticated user, and session-bound CSRF material in a secret-bearing result with redacted debug output.

### Authentication workflow

Perform these checks for every private request:

1. Extract the configured cookie only; never fall back to bearer or query tokens.
2. Validate JWT signature and expiration through Loco.
3. Require exactly one string `pid` and one string custom `jti` claim.
4. Reject malformed identifiers before database access.
5. Hash `jti` and find an unrevoked session whose expiry is later than the injected current time.
6. Require the session's user reference to equal JWT `pid`.
7. Load the user and require `active == true`.
8. Return `AuthenticatedUser` without exposing token material.

JWT validation failure, absent session, expired session, revoked session, user mismatch, missing user, and inactive user all become an unauthenticated result. A database timeout or query failure becomes an unavailable result.

### Logout workflow

- Attempt JWT validation and registry revocation when a cookie exists.
- Treat an absent, malformed, expired, already-revoked, or already-deleted session as an idempotent logout success.
- Propagate a real SurrealDB failure so the caller can avoid pretending server-side revocation succeeded.
- Always instruct the controller to clear the browser cookie, including on malformed or expired JWTs.
- Never log the cookie or token when reporting a revocation failure.

### Cookie construction

Centralize cookie construction and deletion. Login creates exactly one session cookie with the configured name, `HttpOnly`, `SameSite=Lax`, `Path=/`, no `Domain`, `Max-Age=12h`, and `Secure` according to validated environment policy. Logout clears the same cookie name and path using an immediately expired cookie.

### Acceptance criteria

- [ ] Login uses Loco password and JWT facilities and persists the registry session before returning a token.
- [ ] JWT `pid`, custom `jti`, registry user, and loaded user must agree.
- [ ] Each login generates a new independent session and JWT.
- [ ] Copied JWTs stop authenticating immediately after registry revocation.
- [ ] Authentication distinguishes unavailable infrastructure from invalid authentication internally.
- [ ] Logout is idempotent while preserving real revocation failures.
- [ ] Cookie construction and deletion are centralized and environment-safe.
- [ ] Secret-bearing values use redacted debug representations.

### Verification

```bash
cargo check
cargo test auth_service_login
cargo test auth_service_authenticate
cargo test auth_service_logout
cargo test auth_cookie
```

---

## Issue 5 — Add current-user extractors, route guards, and CSRF enforcement

**Priority:** High
**Dependencies:** Issue 4

### Objective

Provide reusable Loco/Axum request primitives that make authentication and CSRF protection mandatory and consistent without duplicating security decisions in controllers.

### Authentication extractors

Implement:

- `CurrentUser`: requires a valid cookie, JWT, active registry session, and active user. On success it exposes `AuthenticatedUser` to the handler.
- `OptionalCurrentUser`: distinguishes `Absent`, `Authenticated`, and `StaleCredential` states. A malformed, expired, revoked, or otherwise invalid presented cookie becomes `StaleCredential`, never an authenticated or silently absent user. The guest-only login handler must clear that stale cookie and render normally so a broken browser cookie cannot trap the user outside the login flow. Authentication-infrastructure failures remain errors and return `503`.

Both extractors must retrieve `AuthService` from application state or the shared store. They must not construct services or database clients per request.

### Browser response behavior

- Private HTML requests without valid authentication redirect to `/login` with HTTP `303` and an optional validated `next` destination.
- Private HTMX requests set `HX-Redirect: /login?...` rather than returning a login fragment into an unrelated page region.
- Guest-only `GET /login` redirects authenticated users to the protected landing page, renders normally when no cookie exists, and clears any `StaleCredential` cookie before rendering.
- Service-unavailable authentication failures return a stable `503` page or fragment and never redirect as though the user merely signed out.
- Any response varying by `HX-Request` includes `Vary: HX-Request` without discarding existing `Vary` values.

### Safe `next` destinations

Accept only an absolute-path reference beginning with one `/`. Reject values that:

- Include a scheme or authority.
- Begin with `//` or a backslash variant.
- Contain control characters.
- Target `/login` or `/logout` and would create a redirect loop.

Invalid or missing values fall back to `/`. Do not use host headers to construct the destination.

### CSRF model

Implement two related token modes:

1. **Pre-authentication login token:** `GET /login` creates a random pre-authentication nonce cookie and a signed, expiring token bound to that nonce, the `POST /login` action, and a short lifetime. The cookie is not promoted into the authenticated session.
2. **Authenticated session token:** after login, generate a signed CSRF token bound to the authenticated session's `jti`, the configured origin, and its expiry. Render it into a `<meta name="csrf-token">` value and hidden `_csrf` form fields.

Use audited HMAC and constant-time comparison facilities. Do not design a new hash or signature algorithm. CSRF signing uses the distinct `AuthSettings` secret.

### Enforcement rules

- Require CSRF validation for every application-owned `POST`, `PUT`, `PATCH`, and `DELETE` browser route, including login and logout.
- Accept the token from exactly one of `_csrf` form data or `X-CSRF-Token`; reject ambiguous requests containing conflicting values.
- Validate token signature, expiry, intended action, and binding before invoking the controller workflow.
- Validate `Origin` against the configured canonical origin when present.
- When `Origin` is absent, require a matching same-origin `Referer`; reject the request when neither is available.
- Never place CSRF tokens in query strings, route parameters, response headers, logs, or error pages.
- A CSRF failure returns `403`, does not invoke the target service, and produces a new safe page on the next `GET`.
- Configure the self-hosted HTMX integration to copy the meta token into `X-CSRF-Token` for unsafe same-origin requests only.
- Keep hidden form fields so the same action works without HTMX.

### Acceptance criteria

- [ ] Private controllers can require `CurrentUser` through a handler parameter.
- [ ] Guest-only pages use `OptionalCurrentUser` without accepting invalid credentials or allowing a stale cookie to block access to login.
- [ ] Browser and HTMX unauthorized responses navigate to login correctly.
- [ ] External, protocol-relative, malformed, and looping `next` destinations are rejected.
- [ ] Login CSRF state is discarded and replaced after successful authentication.
- [ ] All unsafe routes reject missing, invalid, expired, incorrectly bound, or cross-origin tokens.
- [ ] Standard HTML and HTMX submit the same CSRF guarantee.
- [ ] CSRF tokens never appear in URLs or logs.

### Verification

```bash
cargo test current_user_extractor
cargo test auth_redirects
cargo test safe_next_destination
cargo test csrf
```

---

## Issue 6 — Build the server-rendered login and logout flow

**Priority:** High
**Dependencies:** Issues 3–5

### Objective

Deliver an accessible login and logout experience that uses the shared authentication service and works identically with normal HTML navigation and HTMX enhancement.

### Routes

| Method and path | Access | Behavior |
| --- | --- | --- |
| `GET /login` | Guest only | Render the complete login page and issue fresh pre-authentication CSRF state. |
| `POST /login` | Guest only, valid login CSRF | Validate credentials, create the session, set the cookie, and redirect safely. |
| `POST /logout` | Authenticated, valid session CSRF | Revoke the current session, clear the cookie, and redirect to login. |

There is no `GET /logout` and no registration route.

### Controller requirements

- Controllers parse HTTP input, call `AuthService`, select a response, and contain no password, JWT, session, throttle, or database logic.
- Accept `application/x-www-form-urlencoded` for normal and HTMX login submissions.
- Enforce conservative form body limits before password hashing.
- On normal success, return `303 See Other` with `Location` set to the validated `next` path or `/`.
- On HTMX success, return a response with `HX-Redirect` to the same destination and `Vary: HX-Request`.
- On invalid fields, re-render with HTTP `422` and field-level errors that do not echo the password.
- On credential failure, return the same visible message and similar response shape for unknown, wrong-password, and inactive users.
- On temporary throttling, return `429`, a generic retry message, and a bounded `Retry-After` header without revealing account existence.
- On authentication infrastructure failure, return `503` with a correlation identifier and no raw error.
- Logout must clear the cookie on successful, absent, expired, malformed, and already-revoked sessions. A genuine registry failure returns `503` and a cleared cookie while clearly stating that server-side logout could not be confirmed.

### Templates and typed view data

Create typed view models and templates for:

- Complete login page.
- Login form fragment used by HTMX error responses.
- Authentication unavailable page or fragment.
- Reusable inline error summary.

The login form must include:

- A unique page heading and concise instruction.
- A visible email label and `type="email"`, `autocomplete="username"`, `inputmode="email"`, and autofocus behavior that does not disrupt error review.
- A visible password label and `type="password"`, `autocomplete="current-password"`.
- Hidden `_csrf` and validated `next` fields.
- Accessible inline errors connected with `aria-describedby` and an error summary announced through an appropriate live region.
- A submit button with a visible HTMX loading state and disabled-state protection against accidental repeated submission.
- No password value in returned HTML under any outcome.

The page must remain fully usable at a 320-pixel viewport and with JavaScript disabled. Do not add a registration, forgot-password, social-login, or remember-me control.

### HTMX behavior

- Enhance only the form submission and error region; do not require HTMX for navigation or security.
- Use the existing self-hosted HTMX asset.
- Send the CSRF header through the centralized HTMX configuration from Issue 5.
- Return only the form fragment for validation or credential errors when `HX-Request: true`.
- Return a complete page to normal requests.
- Preserve entered email and validated `next`; never preserve password.

### Acceptance criteria

- [ ] `GET /login` renders a complete, accessible page for guests.
- [ ] Authenticated users cannot return to the guest-only login page.
- [ ] Valid credentials create the cookie and navigate to the protected destination.
- [ ] Invalid input, credentials, throttle, and infrastructure failure have stable safe responses.
- [ ] Passwords are never echoed into HTML, logs, URLs, or redirects.
- [ ] Logout is POST-only, CSRF-protected, revokes the registry session, and clears the cookie.
- [ ] Normal HTML and HTMX paths provide equivalent security and outcomes.
- [ ] The form works without JavaScript and at a 320-pixel viewport.

### Verification

```bash
cargo test login_requests
cargo test logout_requests
cargo loco routes
cargo loco start
```

Manually verify login and logout once with JavaScript enabled and once with JavaScript disabled.

---

## Issue 7 — Protect the application shell and harden authenticated responses

**Priority:** High
**Dependencies:** Issues 5 and 6

### Objective

Make authentication the default server-side boundary for private Pipauto pages and apply response hardening consistently across the application shell.

### Access-control requirements

- Change `GET /` from the public setup page into the authenticated landing page.
- Require `CurrentUser` on the handler itself or apply an equivalently explicit route-level guard that cannot be bypassed by calling the route directly.
- Keep infrastructure health endpoints public only when their existing contract requires it; they must continue returning non-sensitive machine health data.
- Keep static assets public.
- Classify every registered route as public, guest-only, or authenticated in `docs/authentication.md`.
- Add a test that enumerates `cargo loco routes` output or the application route registry and fails when a newly added business route has no declared access class.
- Do not rely on hiding navigation links, HTMX headers, JavaScript redirects, or template conditions for authorization.

### Authenticated application shell

Extend the typed layout data with:

- Presentation-safe current user.
- Authenticated-session CSRF token.
- Current path needed for navigation state, excluding secrets.

Add a visible logout form using `POST /logout` and a hidden `_csrf` field. It must work without HTMX. Do not display the user's email if the display name is sufficient for the shell; never display internal record or session identifiers.

### Response hardening

- Add `Cache-Control: no-store` to login responses and all authenticated HTML or HTML fragments.
- Add `Pragma: no-cache` only if required for older clients; do not use it instead of `no-store`.
- Ensure session-creating responses cannot be cached.
- Enable and verify Loco's secure-headers middleware with a policy compatible with self-hosted CSS, JavaScript, Tera output, and HTMX.
- Avoid broad inline-script allowances. Prefer the existing self-hosted script and external application script.
- Set `Referrer-Policy`, `X-Content-Type-Options`, clickjacking protection through CSP `frame-ancestors` or the supported header, and a restrictive permissions policy.
- Preserve `Vary: HX-Request` wherever page and fragment representations differ.
- Ensure error pages use the same secret-redaction and caching policy.

### Login throttling behavior

- Enforce the repository-backed limits selected in Issue 2 before expensive password verification when a temporary block is active.
- Do not create a permanent account lock.
- Use the same externally observable response for known and unknown email addresses.
- Reset or expire counters predictably after successful authentication or the configured window.
- Record a structured security event without raw email, raw IP, password, JWT, session ID, or CSRF data.
- Do not trust `X-Forwarded-For` unless Loco's remote-IP middleware is explicitly configured with trusted proxies for the deployment.

### Acceptance criteria

- [ ] Direct unauthenticated `GET /` requests cannot reach the landing controller output.
- [ ] Every non-static route has a documented access class.
- [ ] The authenticated shell shows the current user and a CSRF-protected logout action.
- [ ] Login and authenticated HTML responses use `Cache-Control: no-store`.
- [ ] Security headers are present and compatible with the self-hosted frontend.
- [ ] HTMX and normal responses retain correct `Vary` behavior.
- [ ] Throttling is temporary, consistent for known and unknown users, and free of trusted-proxy spoofing assumptions.

### Verification

```bash
cargo test protected_routes
cargo test authenticated_layout
cargo test security_headers
cargo test login_throttling
cargo loco routes
```

Use normal and HTMX requests to confirm that unauthenticated access navigates to login and authenticated access returns the protected page.

---

## Issue 8 — Complete authentication verification and developer documentation

**Priority:** Medium
**Dependencies:** Issues 1–7

### Objective

Prove the entire authentication boundary from a clean checkout and provide exact operational instructions for development and production deployment.

### Documentation requirements

Complete the README and `docs/authentication.md` with:

- The resolved Loco version and links to the framework APIs used.
- The JWT-plus-SurrealDB-registry request flow.
- Required authentication environment variables and safe secret-generation commands.
- Development versus production cookie names and properties.
- The fixed 12-hour session lifetime and absence of refresh or remember-me behavior.
- The exact first-user creation command, including the confirmed Loco task-variable syntax.
- The fact that password entry occurs through a non-echoing interactive terminal prompt.
- Login, logout, protected route, and public route behavior.
- CSRF handling for standard forms and HTMX.
- Session revocation and expired-session cleanup commands.
- How to deactivate a compromised account and revoke all of its sessions through a documented administrative procedure or task.
- Production HTTPS and reverse-proxy requirements, including trusted-proxy configuration if applicable.
- A troubleshooting section covering invalid secrets, insecure production origin, cookie-name mismatch, system clock problems, failed user creation, invalid credentials, throttling, revoked/expired sessions, CSRF failures, unavailable SurrealDB, missing templates, and bad proxy headers.

Commands must match the committed application and be run once before the issue is considered complete. Do not document a password in a shell argument or environment variable.

### Automated verification matrix

Ensure coverage exists for:

| Area | Required scenarios |
| --- | --- |
| Settings | Valid development and production config; short/equal/missing secrets; HTTP production origin; unsafe cookie configuration; redacted errors. |
| Users | Email normalization; unique email; valid create; duplicate create; inactive user; password never present in normal projections. |
| Passwords | Boundary lengths; Argon2id PHC output; correct/wrong password; dummy verification for unknown user; no secret snapshots. |
| Sessions | Unique login sessions; matching JWT and registry expiry; absent, expired, revoked, and mismatched sessions; idempotent concurrent logout. |
| JWT | Valid token; tampered signature; expired token; missing or malformed `pid`; missing, malformed, or duplicate `jti`; wrong configured cookie. |
| Cookies | Development and production names; `HttpOnly`; `Secure`; `SameSite=Lax`; `Path=/`; no `Domain`; correct `Max-Age`; exact deletion attributes. |
| CSRF | Valid normal form and HTMX header; missing/expired/tampered token; wrong action/session/origin; conflicting form/header values; login session rotation. |
| Redirects | Valid relative `next`; external URL; protocol-relative URL; control characters; login/logout loops; HTMX `HX-Redirect`. |
| Controllers | Successful login/logout; invalid form; invalid credentials; inactive user; throttle; repository outage; no password echo. |
| Access control | Guest login page; authenticated guest-only redirect; unauthenticated private route; authenticated landing page; direct route bypass attempt. |
| Presentation | Full page versus fragment; `Vary: HX-Request`; accessible error linkage; JavaScript-free forms; 320-pixel layout. |
| Hardening | `Cache-Control: no-store`; secure headers; secret-free logs and errors; untrusted forwarding headers ignored. |

Use deterministic clocks and injected secure-random substitutes in tests. Production code must continue using the system clock and operating-system CSPRNG.

### Manual end-to-end verification

From a clean checkout:

1. Configure safe development environment values.
2. Start and health-check SurrealDB.
3. Apply the authentication schema.
4. Create one user through the interactive Loco task.
5. Start Pipauto.
6. Confirm unauthenticated `/` navigation reaches `/login`.
7. Submit wrong credentials and confirm the generic safe response.
8. Sign in normally and confirm the protected shell and cookie properties.
9. Sign out and confirm the old JWT no longer works even if replayed.
10. Repeat login and logout with HTMX disabled.
11. Temporarily stop SurrealDB and confirm authentication returns a safe unavailable response instead of bypassing the guard.

### Milestone acceptance checklist

- [ ] A clean checkout can configure authentication using only committed documentation.
- [ ] A user can be created without exposing a password in command history, environment, logs, or output.
- [ ] Valid credentials establish a 12-hour JWT-backed, SurrealDB-revocable session.
- [ ] Invalid credentials, inactive users, and unknown users share a generic response.
- [ ] Logout immediately invalidates the current registry session and clears the cookie.
- [ ] Every private route enforces authentication on the server.
- [ ] Every unsafe browser route enforces CSRF protection.
- [ ] Normal forms and HTMX provide equivalent functionality and security.
- [ ] Production configuration requires HTTPS-safe cookie and origin settings.
- [ ] Tests cover the complete verification matrix.
- [ ] No public registration, recovery, email verification, social login, MFA, API key, remember-me, role, or granular permission feature is included.

### Final verification gate

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo loco routes
cargo loco task
```

Then complete the manual end-to-end verification above and inspect logs to confirm that no authentication secret or credential was emitted.

## Authentication interfaces and invariants

This milestone exposes these application interfaces:

- `AuthSettings`: validated, secret-redacting authentication configuration.
- `User`, `UserId`, `NormalizedEmail`, and internal `UserCredentials`.
- `AuthSession`, `SessionDigest`, and typed session creation/revocation outcomes.
- `UserRepository`, `AuthSessionRepository`, and login-throttle repository contracts.
- `AuthService`: login, authentication, and logout workflows.
- `AuthenticatedUser`: presentation-safe current-user state.
- `CurrentUser` and `OptionalCurrentUser`: Loco/Axum request extractors.
- `GET /login`: guest-only server-rendered login page.
- `POST /login`: CSRF-protected credential submission.
- `POST /logout`: authenticated, CSRF-protected session revocation.
- `GET /`: authenticated landing page.
- `create_user`: interactive Loco administration task.
- `purge_expired_auth_sessions`: session cleanup task.

All implementation work must preserve these invariants:

1. A JWT is necessary but never sufficient; its active SurrealDB registry record and active user are also required.
2. Raw `jti` values, JWTs, passwords, password hashes, JWT secrets, and CSRF secrets never enter logs or normal persistence projections.
3. Controllers contain no authentication policy, password hashing, JWT operations, or database queries.
4. Repositories contain persistence behavior but no HTTP, cookie, redirect, template, or credential-verification behavior.
5. Authentication infrastructure failures never become successful anonymous access and never masquerade as invalid credentials internally.
6. Session expiry is fixed at issuance and cannot be extended by request activity.
7. Logout is idempotent for absent or already-invalid sessions but does not hide real registry failures.
8. Unsafe methods require CSRF validation before their application workflow runs.
9. Authorization is enforced by the server route boundary, never by navigation visibility or HTMX behavior.
10. All dynamic SurrealQL values use bound parameters.

## Technical assumptions

- The completed Project Setup milestone and its documented module boundaries remain authoritative.
- Loco 0.16.x database-independent JWT APIs are available or will be adopted through a Cargo-managed dependency update.
- Local development may use HTTP; production authentication requires HTTPS.
- Application users are records managed through Pipauto repositories, not SurrealDB system users or record-authentication sessions.
- One or more active users may exist, but all have identical application access in this milestone.
- Automated persistence tests use the SurrealDB in-memory engine unless explicitly verifying the standalone service.
- The existing self-hosted HTMX and Tera infrastructure is reused without Node, npm, a bundler, or a CDN.
- Session cleanup is performed through a registered task until a later milestone deliberately introduces scheduling.
