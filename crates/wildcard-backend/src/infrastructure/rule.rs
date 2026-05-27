use crate::{
    domain::rule::{PublishedRule, RuleDraft, RuleId, RuleStatus},
    error::AppError,
};
use sqlx::PgPool;
use tracing::error;

#[derive(Debug)]
pub struct RuleRepository {
    pub pg_pool: PgPool,
}

impl RuleRepository {
    pub async fn save_draft_rule(&self, draft: &RuleDraft) -> Result<(), AppError> {
        let design = serde_json::to_value(&draft.design).map_err(AppError::Json)?;
        let status = match draft.status {
            RuleStatus::Draft => "draft",
            RuleStatus::Published => "published",
        };

        sqlx::query!(
            r#"
            INSERT INTO rule_drafts (
                id, owner_id, name, player_count, description, status, design,
                published_rule_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                player_count = EXCLUDED.player_count,
                description = EXCLUDED.description,
                status = EXCLUDED.status,
                design = EXCLUDED.design,
                updated_at = DEFAULT
            "#,
            draft.id.0,
            draft.owner_id.0,
            draft.name,
            draft.player_count as i16,
            draft.description,
            status,
            design,
            draft.published_rule_id
        )
        .execute(&self.pg_pool)
        .await
        .inspect_err(|e| error!("Database error {e}"))?;

        Ok(())
    }

    pub async fn save_published_rule(
        &self,
        rule: &PublishedRule,
        draft_id: &RuleId,
    ) -> Result<(), AppError> {
        let design = serde_json::to_value(&rule.design).map_err(AppError::Json)?;

        sqlx::query!(
            r#"
            INSERT INTO rule_published (
                id, draft_id, owner_id, name, player_count, description, version,
                design
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                player_count = EXCLUDED.player_count,
                description = EXCLUDED.description,
                version = rule_published.version + 1,
                design = EXCLUDED.design,
                updated_at = DEFAULT
            "#,
            rule.id.0,
            draft_id.0,
            rule.owner_id.0,
            rule.name,
            rule.player_count as i16,
            rule.description,
            rule.version as i32,
            design
        )
        .execute(&self.pg_pool)
        .await
        .inspect_err(|e| error!("Database error {e}"))?;

        Ok(())
    }

    pub async fn delete_draft(&self, draft_id: &RuleId) -> Result<(), AppError> {
        sqlx::query!("DELETE FROM rule_published WHERE draft_id = $1", draft_id.0)
            .execute(&self.pg_pool)
            .await
            .inspect_err(|e| error!("Database error {e}"))?;

        sqlx::query!("DELETE FROM rule_drafts WHERE id = $1", draft_id.0)
            .execute(&self.pg_pool)
            .await
            .inspect_err(|e| error!("Database error {e}"))?;

        Ok(())
    }
}
