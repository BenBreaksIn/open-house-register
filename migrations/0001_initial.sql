CREATE TABLE IF NOT EXISTS settings (
    id BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (id),
    data JSONB NOT NULL
);
CREATE TABLE IF NOT EXISTS open_houses (
    id UUID PRIMARY KEY,
    address TEXT NOT NULL,
    location TEXT NOT NULL,
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    timezone TEXT NOT NULL,
    photo_url TEXT NOT NULL DEFAULT '',
    note TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL CHECK (status IN ('draft', 'open', 'closed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (ends_at > starts_at)
);
CREATE TABLE IF NOT EXISTS visitors (
    id UUID PRIMARY KEY,
    house_id UUID NOT NULL REFERENCES open_houses(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    email TEXT NOT NULL,
    phone TEXT NOT NULL DEFAULT '',
    timeline TEXT NOT NULL DEFAULT '',
    represented TEXT NOT NULL DEFAULT '',
    follow_up BOOLEAN NOT NULL DEFAULT FALSE,
    consent_text TEXT NOT NULL,
    checked_in_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (house_id, email)
);
CREATE INDEX IF NOT EXISTS visitors_house_time ON visitors(house_id, checked_in_at DESC);
CREATE TABLE IF NOT EXISTS sessions (
    token_hash TEXT PRIMARY KEY,
    password_fingerprint TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS rate_limits (
    bucket TEXT PRIMARY KEY,
    window_start TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    attempts INTEGER NOT NULL DEFAULT 1
);
