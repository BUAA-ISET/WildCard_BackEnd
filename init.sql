CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY,
    name VARCHAR(255) UNIQUE NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    password VARCHAR(255) NOT NULL,
    avatar VARCHAR(512) NOT NULL DEFAULT '',
    role VARCHAR(16) NOT NULL DEFAULT 'user' CHECK (role IN ('user', 'admin'))
);

ALTER TABLE users ADD COLUMN IF NOT EXISTS avatar VARCHAR(512) NOT NULL DEFAULT '';
ALTER TABLE users ADD COLUMN IF NOT EXISTS role VARCHAR(16) NOT NULL DEFAULT 'user';

CREATE INDEX IF NOT EXISTS idx_users_name ON users(name);
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);

CREATE TABLE IF NOT EXISTS rule_drafts (
    id UUID PRIMARY KEY,
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    player_count SMALLINT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status VARCHAR(32) NOT NULL DEFAULT 'draft',
    design JSONB NOT NULL,
    published_rule_id VARCHAR(128),
    forked_from_rule_id VARCHAR(128),
    reject_reason TEXT,
    introduction TEXT NOT NULL DEFAULT '',
    cover_url VARCHAR(512) NOT NULL DEFAULT '',
    screenshot_urls JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

ALTER TABLE rule_drafts ADD COLUMN IF NOT EXISTS forked_from_rule_id VARCHAR(128);
ALTER TABLE rule_drafts ADD COLUMN IF NOT EXISTS reject_reason TEXT;
ALTER TABLE rule_drafts ADD COLUMN IF NOT EXISTS introduction TEXT NOT NULL DEFAULT '';
ALTER TABLE rule_drafts ADD COLUMN IF NOT EXISTS cover_url VARCHAR(512) NOT NULL DEFAULT '';
ALTER TABLE rule_drafts ADD COLUMN IF NOT EXISTS screenshot_urls JSONB NOT NULL DEFAULT '[]'::jsonb;

CREATE INDEX IF NOT EXISTS idx_rule_drafts_owner_updated
    ON rule_drafts(owner_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS rule_published (
    id UUID PRIMARY KEY,
    draft_id UUID REFERENCES rule_drafts(id) ON DELETE SET NULL,
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    player_count SMALLINT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    version INTEGER NOT NULL DEFAULT 1,
    design JSONB NOT NULL,
    introduction TEXT NOT NULL DEFAULT '',
    cover_url VARCHAR(512) NOT NULL DEFAULT '',
    screenshot_urls JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

ALTER TABLE rule_published ADD COLUMN IF NOT EXISTS introduction TEXT NOT NULL DEFAULT '';
ALTER TABLE rule_published ADD COLUMN IF NOT EXISTS cover_url VARCHAR(512) NOT NULL DEFAULT '';
ALTER TABLE rule_published ADD COLUMN IF NOT EXISTS screenshot_urls JSONB NOT NULL DEFAULT '[]'::jsonb;

CREATE INDEX IF NOT EXISTS idx_rule_published_owner_updated
    ON rule_published(owner_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS match_replays (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    room_code TEXT NOT NULL,
    player_ids TEXT[] NOT NULL,
    replay JSONB NOT NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_match_replays_player_ids
    ON match_replays USING GIN(player_ids);

CREATE INDEX IF NOT EXISTS idx_match_replays_updated
    ON match_replays(updated_at DESC);

CREATE TABLE IF NOT EXISTS rule_reviews (
    id UUID PRIMARY KEY,
    rule_id UUID NOT NULL,
    author_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    rating SMALLINT NOT NULL CHECK (rating >= 1 AND rating <= 5),
    content TEXT NOT NULL DEFAULT '',
    image_url VARCHAR(512),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (rule_id, author_id)
);

CREATE INDEX IF NOT EXISTS idx_rule_reviews_rule_created
    ON rule_reviews(rule_id, created_at DESC);

CREATE TABLE IF NOT EXISTS reports (
    id UUID PRIMARY KEY,
    reporter_id VARCHAR(128) NOT NULL,
    reporter_name VARCHAR(255) NOT NULL DEFAULT '',
    reporter_avatar VARCHAR(512) NOT NULL DEFAULT '',
    target_type VARCHAR(32) NOT NULL,
    target_id VARCHAR(255) NOT NULL,
    reason TEXT NOT NULL DEFAULT '',
    details TEXT NOT NULL DEFAULT '',
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    context JSONB NOT NULL DEFAULT '{}'::jsonb,
    action_log JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_reports_status_created
    ON reports(status, created_at DESC);

-- 首任管理员：保证 Tanhhhhtjy 始终拥有 admin 角色（手动改回 user 不会被覆盖，
-- 因为应用启动时使用了 AND role <> 'admin' 守卫；init.sql 仅用于全新部署）。
UPDATE users SET role = 'admin' WHERE name = 'Tanhhhhtjy';
