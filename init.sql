CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY,
    name VARCHAR(255) UNIQUE NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    password VARCHAR(255) NOT NULL,
    avatar VARCHAR(512) NOT NULL DEFAULT ''
);

ALTER TABLE users ADD COLUMN IF NOT EXISTS avatar VARCHAR(512) NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_users_name ON users(name);
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);

CREATE TABLE IF NOT EXISTS rule_drafts (
    id VARCHAR(128) PRIMARY KEY,
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    player_count SMALLINT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status VARCHAR(32) NOT NULL DEFAULT 'draft',
    design JSONB NOT NULL,
    published_rule_id VARCHAR(128),
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_rule_drafts_owner_updated
    ON rule_drafts(owner_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS rule_published (
    id VARCHAR(128) PRIMARY KEY,
    draft_id VARCHAR(128) REFERENCES rule_drafts(id) ON DELETE SET NULL,
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    player_count SMALLINT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    version INTEGER NOT NULL DEFAULT 1,
    design JSONB NOT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_rule_published_owner_updated
    ON rule_published(owner_id, updated_at DESC);
