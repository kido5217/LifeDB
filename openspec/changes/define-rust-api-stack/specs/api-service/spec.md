# Spec Delta

## Purpose

Defines the behavioral contract of the LifeDB REST service: how it starts, configures, logs, reports errors, protects and version its surface, and shuts down.

## ADDED Requirements

### Requirement: Fail-fast environment configuration
The service SHALL load all runtime configuration from environment variables at startup and SHALL refuse to start — exiting nonzero with a message naming every missing or invalid variable — before binding its listen address.

#### Scenario: Missing database URL
- **WHEN** the service starts without `DATABASE_URL` set
- **THEN** it exits nonzero before opening a listener
- **AND** the error output names `DATABASE_URL`

#### Scenario: Invalid numeric setting among several
- **WHEN** the service starts with a non-numeric timeout and an unset origin list
- **THEN** it exits nonzero and reports both problems in one message

### Requirement: Migrations applied before traffic
The service SHALL apply all pending schema migrations to its database before accepting any request, and a migration failure SHALL abort startup.

#### Scenario: Empty database
- **WHEN** the service starts against a database with no LifeDB tables
- **THEN** startup completes and the account tables exist before the first request is served

#### Scenario: Unreachable database
- **WHEN** the configured database cannot be reached at startup
- **THEN** the service exits nonzero with an error naming the database endpoint

### Requirement: Structured request logging
The service SHALL emit one structured log event per completed request carrying method, path, status code, duration, and the request's correlation id; the event format SHALL be human-readable text or JSON as configured, and the minimum level SHALL be environment-controllable.

#### Scenario: Served request
- **WHEN** any request completes
- **THEN** exactly one log event contains its method, path, status, duration, and request id

#### Scenario: JSON format selected
- **WHEN** the service runs with the log format configured for production
- **THEN** request log lines parse as JSON objects containing those fields

### Requirement: Request correlation id
The service SHALL assign a unique correlation id to every incoming request and MUST expose it in the response headers and in every log event and error body produced for that request.

#### Scenario: Response correlation
- **WHEN** a client sends any request
- **THEN** the response carries a request-id header whose value appears in the matching log event

### Requirement: Consistent error envelope
Every error response the service generates SHALL use a single JSON body shape containing a client-safe message and the request's correlation id; internal failure details MUST be logged and MUST NOT appear in the response body.

#### Scenario: Internal failure does not leak
- **WHEN** a handler fails because the database connection was lost
- **THEN** the client receives a 5xx response with the generic envelope
- **AND** the response body contains no database error text
- **AND** the logged event contains the database error and the same correlation id

#### Scenario: Malformed request body
- **WHEN** a client sends a body that cannot be deserialized
- **THEN** the client receives a 4xx response using the same envelope

### Requirement: Versioned API surface with generated OpenAPI
All functional endpoints SHALL live under the `/api/v1` prefix, and the service SHALL serve an OpenAPI document at `/openapi.json` generated from its registered routes and request/response types.

#### Scenario: Document completeness
- **WHEN** a client fetches `/openapi.json`
- **THEN** the document lists every registered `/api/v1` endpoint with its request and response schemas

#### Scenario: Unknown versioned path
- **WHEN** a client requests an undefined path under `/api/v1`
- **THEN** the service returns a 404 using the error envelope

### Requirement: Liveness and readiness probes
The service SHALL expose a liveness probe that depends on nothing and a readiness probe that reports ready only when the database is reachable.

#### Scenario: Database goes away after startup
- **WHEN** the database becomes unreachable while the service runs
- **THEN** the liveness probe still succeeds
- **AND** the readiness probe reports not-ready with a 5xx status

### Requirement: Request timeout and body limit
The service SHALL terminate requests that exceed a configured timeout with a client-visible timeout response, and SHALL reject bodies above a configured limit with a 413 using the error envelope.

#### Scenario: Slow handler
- **WHEN** a request runs longer than the configured timeout
- **THEN** the client receives a timeout status instead of hanging indefinitely

#### Scenario: Oversized body
- **WHEN** a request body exceeds the configured limit
- **THEN** the service responds 413 without reading the rest of the body

### Requirement: Cross-origin restriction
The service SHALL include the configured allow-list of browser origins in CORS responses and MUST NOT reflect or allow arbitrary origins.

#### Scenario: Foreign origin preflight
- **WHEN** a preflight arrives from an origin not in the configured list
- **THEN** the response contains no CORS allow-origin match for it

### Requirement: Graceful shutdown
On SIGTERM or SIGINT the service SHALL stop accepting new connections, finish in-flight requests within a bounded drain period, and exit successfully.

#### Scenario: Signal during an in-flight request
- **WHEN** the service receives SIGTERM while a request is being handled
- **THEN** that request completes normally
- **AND** connections opened afterward are not served
- **AND** the process exits 0 within the drain window
