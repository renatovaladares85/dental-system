BEGIN IMMEDIATE;

ALTER TABLE audit_events
    ADD COLUMN result TEXT NOT NULL DEFAULT 'SUCCESS'
    CHECK (result IN ('SUCCESS', 'DENIED', 'FAILURE'));

ALTER TABLE audit_events
    ADD COLUMN correlation_id TEXT
    CHECK (correlation_id IS NULL OR length(correlation_id) = 36);

ALTER TABLE audit_events
    ADD COLUMN session_id TEXT
    REFERENCES sessions(id) ON DELETE RESTRICT;

ALTER TABLE audit_events
    ADD COLUMN source TEXT NOT NULL DEFAULT 'SYSTEM'
    CHECK (source IN ('APPLICATION', 'ADMIN_LOOPBACK', 'LAN', 'SYSTEM'));

CREATE INDEX audit_events_actor_occurred
    ON audit_events (actor_user_id, occurred_at DESC);

CREATE INDEX audit_events_action_occurred
    ON audit_events (action, occurred_at DESC);

CREATE INDEX audit_events_correlation
    ON audit_events (correlation_id)
    WHERE correlation_id IS NOT NULL;

UPDATE installations SET schema_version = 3 WHERE schema_version = 2;

PRAGMA user_version = 3;

COMMIT;
