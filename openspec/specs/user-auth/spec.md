# user-auth Specification

## Purpose

Defines account and session behavior for the browser-only LifeDB client: how credentials are stored, how sessions are issued, validated and revoked, and how the cookie surface is protected.

## Requirements

### Requirement: Argon2id password storage
The service SHALL store each password only as a memory-hard Argon2id hash in a self-describing encoded form that records the hashing parameters, and MUST NOT persist or log the plaintext password.

#### Scenario: Stored form
- **WHEN** an account is created or its password changed
- **THEN** the stored value is an Argon2id encoded hash string and contains no plaintext

#### Scenario: Parameter upgrade transparency
- **WHEN** a password was hashed with older embedded parameters
- **THEN** verification still succeeds because the stored string carries its own parameters

### Requirement: Account registration
The service SHALL let an unauthenticated browser create an account with a unique username, a unique email, and a password meeting the configured strength policy; input that fails validation SHALL receive a 400 and a taken username or email SHALL receive a 409. Registration SHALL NOT sign the client in. Login SHALL continue to accept only username and password.

#### Scenario: New username
- **WHEN** a valid registration request arrives
- **THEN** the account is stored (username, email, password hash) and the response is a success status without a session cookie

#### Scenario: Duplicate username
- **WHEN** a registration arrives for an existing username
- **THEN** the response is 409 and no second row is created

#### Scenario: Duplicate email
- **WHEN** a registration arrives with an email that differs from an existing account's only in letter case
- **THEN** the response is 409 and no second row is created

#### Scenario: Weak password
- **WHEN** a registration arrives with a password shorter than the policy minimum
- **THEN** the response is 400 with the error envelope listing the failed rules

#### Scenario: Missing or malformed email
- **WHEN** a registration arrives with no email or an email failing the format or 254-character limit
- **THEN** the response is 400 with the error envelope listing the failed rules

#### Scenario: Login payload unchanged
- **WHEN** a login posts exactly username and password to an account that was registered with an email
- **THEN** the session cookie is set as before (login never requires or reads an email field)


### Requirement: Session login with hardened cookie
On valid credentials the service SHALL create a session and set a session cookie that is `HttpOnly`, `SameSite=Lax`, and marked `Secure` in production; invalid credentials SHALL yield 401 with the error envelope.

#### Scenario: Correct credentials
- **WHEN** a client posts the right username and password
- **THEN** the response sets a session cookie with `HttpOnly` and `SameSite=Lax`
- **AND** the current-user endpoint returns that account while the cookie is presented

#### Scenario: Wrong password
- **WHEN** a client posts an existing username with a wrong password
- **THEN** the response is 401 and no session is created

### Requirement: Timing-safe credential check
A login attempt for a non-existent username SHALL take measurably comparable work to a wrong-password attempt and SHALL return the same 401 response, so callers cannot enumerate usernames by response body, status, or timing.

#### Scenario: Unknown username
- **WHEN** a login arrives for a username that does not exist
- **THEN** the response is the same generic 401 used for wrong passwords
- **AND** the server still performs a hash verification against a dummy hash

### Requirement: Session validation and derivation of identity
Protected endpoints SHALL derive the acting user exclusively from the presented session; a missing, unknown, expired, or foreign session cookie SHALL yield 401, and the session's own stored identity — not any client-declared field — SHALL determine the acting user.

#### Scenario: Tampered cookie
- **WHEN** a request presents a modified session cookie value
- **THEN** the response is 401

#### Scenario: Session expiry
- **WHEN** a session older than the configured lifetime is presented
- **THEN** the response is 401 and the session is treated as revoked

### Requirement: Session revocation
Logout SHALL clear the cookie and destroy the session so the same cookie value no longer authenticates. While the session store is in-process only, a server restart SHALL revoke all sessions.

#### Scenario: Logout
- **WHEN** an authenticated client logs out
- **THEN** the response expires the cookie
- **AND** reusing the old cookie at the current-user endpoint returns 401

#### Scenario: Restart invalidates sessions
- **WHEN** the server restarts with a previously valid session cookie
- **THEN** the cookie no longer authenticates and the client receives 401

### Requirement: Origin check on session-changing requests
Every state-changing request (POST, PUT, PATCH, DELETE) bearing a session cookie SHALL be rejected with 403 unless its `Origin` matches the configured allowed origin(s); login and registration are exempt from the session requirement but still origin-checked when an `Origin` header is present.

#### Scenario: Cross-site form post
- **WHEN** a same-cookie POST arrives with a foreign `Origin` header
- **THEN** the response is 403 and the session is unaffected

### Requirement: Email identity storage
Each account SHALL carry an optional email stored case-insensitively unique (comparison and constraint fold letter case; the original input case is preserved for display). Email SHALL NOT function as a login identifier: credential checks resolve accounts by username only.

#### Scenario: Case-folded uniqueness
- **WHEN** two registrations present emails differing only in case, in either arrival order
- **THEN** exactly one row exists for that address

#### Scenario: Login does not consult email
- **WHEN** a login request carries the account's email address in the username field
- **THEN** no email lookup is performed and the response is the standard 400 validation envelope (email-shaped strings fail the username rule), never a different code for account existence

#### Scenario: Legacy rows
- **WHEN** an account predates email capture
- **THEN** it authenticates and is served normally with a null email

### Requirement: Current-user view exposes email
The current-user endpoint SHALL report the authenticated account's email (or null when absent) alongside its username.

#### Scenario: Account with email
- **WHEN** a client registered with `Alice@Example.com` logs in and fetches the current-user view
- **THEN** the response's email is the stored original-case value
