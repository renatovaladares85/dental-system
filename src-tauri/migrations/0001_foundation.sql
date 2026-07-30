BEGIN IMMEDIATE;

CREATE TABLE installations (
    id TEXT PRIMARY KEY NOT NULL,
    singleton INTEGER NOT NULL DEFAULT 1 UNIQUE CHECK (singleton = 1),
    database_id TEXT NOT NULL UNIQUE,
    key_id TEXT NOT NULL UNIQUE,
    setup_state TEXT NOT NULL CHECK (setup_state IN ('RECOVERY_PENDING', 'READY')),
    schema_version INTEGER NOT NULL DEFAULT 1 CHECK (schema_version >= 1),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;

CREATE TABLE organizations (
    id TEXT PRIMARY KEY NOT NULL,
    installation_id TEXT NOT NULL UNIQUE REFERENCES installations(id) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (length(name) BETWEEN 2 AND 120),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;

CREATE TABLE units (
    id TEXT PRIMARY KEY NOT NULL,
    organization_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (length(name) BETWEEN 2 AND 120),
    responsible_name TEXT NOT NULL CHECK (length(responsible_name) BETWEEN 2 AND 120),
    phone TEXT,
    administrative_email TEXT,
    address TEXT,
    professional_registration TEXT,
    is_primary INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (organization_id, name)
) STRICT;

CREATE UNIQUE INDEX units_one_primary_per_organization
    ON units (organization_id)
    WHERE is_primary = 1;

CREATE TABLE users (
    id TEXT PRIMARY KEY NOT NULL,
    installation_id TEXT NOT NULL REFERENCES installations(id) ON DELETE RESTRICT,
    full_name TEXT NOT NULL CHECK (length(full_name) BETWEEN 2 AND 120),
    username TEXT NOT NULL COLLATE NOCASE,
    email TEXT NOT NULL COLLATE NOCASE,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_by_user_id TEXT REFERENCES users(id) ON DELETE RESTRICT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (installation_id, username),
    UNIQUE (installation_id, email)
) STRICT;

CREATE TABLE user_credentials (
    user_id TEXT PRIMARY KEY NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    password_phc TEXT NOT NULL,
    password_changed_at TEXT NOT NULL
) STRICT;

CREATE TABLE roles (
    id TEXT PRIMARY KEY NOT NULL,
    installation_id TEXT NOT NULL REFERENCES installations(id) ON DELETE RESTRICT,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE (installation_id, code)
) STRICT;

CREATE TABLE user_roles (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    assigned_at TEXT NOT NULL,
    PRIMARY KEY (user_id, role_id)
) STRICT;

CREATE TABLE installation_settings (
    installation_id TEXT PRIMARY KEY NOT NULL REFERENCES installations(id) ON DELETE CASCADE,
    backup_directory TEXT NOT NULL,
    recovery_directory TEXT NOT NULL,
    current_recovery_package_id TEXT REFERENCES recovery_package_history(id) ON DELETE RESTRICT,
    current_backup_id TEXT REFERENCES backup_history(id) ON DELETE RESTRICT,
    updated_at TEXT NOT NULL,
    CHECK (
        (current_recovery_package_id IS NULL AND current_backup_id IS NULL)
        OR (current_recovery_package_id IS NOT NULL AND current_backup_id IS NOT NULL)
    )
) STRICT;

CREATE TABLE recovery_package_history (
    id TEXT PRIMARY KEY NOT NULL,
    installation_id TEXT NOT NULL REFERENCES installations(id) ON DELETE RESTRICT,
    file_path TEXT NOT NULL,
    file_name TEXT NOT NULL,
    sha256 TEXT NOT NULL CHECK (length(sha256) = 64),
    created_at TEXT NOT NULL,
    verified_at TEXT
) STRICT;

CREATE INDEX recovery_package_history_installation_created
    ON recovery_package_history (installation_id, created_at DESC);

CREATE TABLE backup_history (
    id TEXT PRIMARY KEY NOT NULL,
    installation_id TEXT NOT NULL REFERENCES installations(id) ON DELETE RESTRICT,
    file_path TEXT NOT NULL,
    file_name TEXT NOT NULL,
    sha256 TEXT NOT NULL CHECK (length(sha256) = 64),
    database_sha256 TEXT NOT NULL CHECK (length(database_sha256) = 64),
    created_at TEXT NOT NULL,
    verified_at TEXT
) STRICT;

CREATE INDEX backup_history_installation_created
    ON backup_history (installation_id, created_at DESC);

CREATE TABLE audit_events (
    id TEXT PRIMARY KEY NOT NULL,
    installation_id TEXT NOT NULL REFERENCES installations(id) ON DELETE RESTRICT,
    actor_type TEXT NOT NULL CHECK (actor_type IN ('SYSTEM', 'USER')),
    actor_user_id TEXT REFERENCES users(id) ON DELETE RESTRICT,
    action TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    occurred_at TEXT NOT NULL,
    CHECK (json_valid(metadata_json)),
    CHECK (
        (actor_type = 'SYSTEM' AND actor_user_id IS NULL)
        OR (actor_type = 'USER' AND actor_user_id IS NOT NULL)
    )
) STRICT;

CREATE INDEX audit_events_installation_occurred
    ON audit_events (installation_id, occurred_at DESC);

CREATE TRIGGER audit_events_append_only_update
BEFORE UPDATE ON audit_events
BEGIN
    SELECT RAISE(ABORT, 'audit_events is append-only');
END;

CREATE TRIGGER audit_events_append_only_delete
BEFORE DELETE ON audit_events
BEGIN
    SELECT RAISE(ABORT, 'audit_events is append-only');
END;

PRAGMA user_version = 1;

COMMIT;
