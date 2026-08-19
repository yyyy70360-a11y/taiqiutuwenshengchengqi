CREATE TABLE IF NOT EXISTS registration_applications (
    id TEXT PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'approved', 'rejected')),
    requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reviewed_at TIMESTAMPTZ,
    reviewed_by TEXT,
    review_note TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS registration_applications_status_requested_idx
    ON registration_applications(status, requested_at DESC);
