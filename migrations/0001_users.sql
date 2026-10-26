-- First account table (design D11). Application supplies the uuid v7 id;
-- the default is only a safety net for manual inserts.
CREATE TABLE users (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    username text NOT NULL UNIQUE COLLATE "C",
    password_hash text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
