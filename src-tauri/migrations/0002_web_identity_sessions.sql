BEGIN IMMEDIATE;

CREATE TABLE sessions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash BLOB NOT NULL UNIQUE CHECK (length(token_hash) = 32),
    csrf_hash BLOB NOT NULL CHECK (length(csrf_hash) = 32),
    created_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    last_rotated_at TEXT NOT NULL,
    idle_expires_at TEXT NOT NULL,
    absolute_expires_at TEXT NOT NULL,
    reauthenticated_at TEXT NOT NULL,
    revoked_at TEXT,
    CHECK (idle_expires_at <= absolute_expires_at),
    CHECK (revoked_at IS NULL OR revoked_at >= created_at)
) STRICT;

CREATE INDEX sessions_user_active
    ON sessions (user_id, absolute_expires_at)
    WHERE revoked_at IS NULL;

CREATE INDEX sessions_expiration
    ON sessions (idle_expires_at, absolute_expires_at)
    WHERE revoked_at IS NULL;

UPDATE installations SET schema_version = 2 WHERE schema_version = 1;

PRAGMA user_version = 2;

COMMIT;
