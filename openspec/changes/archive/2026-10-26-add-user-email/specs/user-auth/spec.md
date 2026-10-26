# Spec Delta

## MODIFIED Requirements

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

## ADDED Requirements

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
