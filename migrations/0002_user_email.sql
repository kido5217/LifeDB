-- add-user-email (design D1): optional, case-insensitively unique email on
-- accounts, plus PG 18 native uuidv7() as the manual-insert id default.
-- Additive-only: existing rows survive, no backfill (column is NULL).
CREATE EXTENSION IF NOT EXISTS citext;

ALTER TABLE users ALTER COLUMN id SET DEFAULT uuidv7();

ALTER TABLE users ADD COLUMN email citext;

-- Uniqueness is partial so NULL (legacy / future no-email accounts) is allowed
-- any number of times; citext's opclass folds letter case.
CREATE UNIQUE INDEX uq_users_email ON users (email) WHERE email IS NOT NULL;
