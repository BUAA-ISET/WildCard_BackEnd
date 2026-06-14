use argon2::Argon2;
use password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng};
use sqlx::{PgPool, Row};

use crate::{
    domain::user::{User, UserId},
    error::AppError,
};

#[derive(Debug)]
pub struct UserRepository {
    pub pool: PgPool,
}

impl UserRepository {
    pub async fn register(&self, user: User) -> Result<(), AppError> {
        sqlx::query("INSERT INTO users (id, name, email, password) VALUES ($1, $2, $3, $4)")
            .bind(user.id.0)
            .bind(&user.name)
            .bind(&user.email)
            .bind(Self::password_hash(&user.password)?)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                if let sqlx::error::Error::Database(db_err) = &e {
                    match db_err.constraint() {
                        Some("users_name_key" | "idx_users_name") => {
                            return AppError::UserAlreadyExist("用户名已存在".to_string());
                        }
                        Some("users_email_key" | "idx_users_email") => {
                            return AppError::UserAlreadyExist("该邮箱已注册".to_string());
                        }
                        Some(other) => {
                            tracing::warn!("Unexpected constraint {other}");
                        }
                        None => {}
                    }
                }
                tracing::warn!("Database error {e}");
                AppError::DatabaseError(e)
            })?;
        Ok(())
    }

    pub async fn find_by_name(&self, name: &str) -> Result<Option<User>, AppError> {
        let user = sqlx::query(
            "SELECT id, name, email, password, avatar, role, banned, banned_until FROM users WHERE users.name = $1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?
        .map(|user| User {
            id: UserId(user.get("id")),
            name: user.get("name"),
            email: user.get("email"),
            password: user.get("password"),
            avatar: user.get("avatar"),
            role: user.get("role"),
            banned: user.get("banned"),
            banned_until: user.get("banned_until"),
        });

        Ok(user)
    }

    pub async fn find_by_email(&self, email: &str) -> Result<Option<User>, AppError> {
        let user = sqlx::query(
            "SELECT id, name, email, password, avatar, role, banned, banned_until FROM users WHERE users.email = $1",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?
        .map(|user| User {
            id: UserId(user.get("id")),
            name: user.get("name"),
            email: user.get("email"),
            password: user.get("password"),
            avatar: user.get("avatar"),
            role: user.get("role"),
            banned: user.get("banned"),
            banned_until: user.get("banned_until"),
        });

        Ok(user)
    }

    pub async fn find_by_id(&self, user_id: &UserId) -> Result<Option<User>, AppError> {
        let user = sqlx::query(
            "SELECT id, name, email, password, avatar, role, banned, banned_until FROM users WHERE users.id = $1",
        )
        .bind(user_id.0)
        .fetch_optional(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?
        .map(|user| User {
            id: UserId(user.get("id")),
            name: user.get("name"),
            email: user.get("email"),
            password: user.get("password"),
            avatar: user.get("avatar"),
            role: user.get("role"),
            banned: user.get("banned"),
            banned_until: user.get("banned_until"),
        });

        Ok(user)
    }

    pub async fn update_username(
        &self,
        user_id: &UserId,
        username: &str,
    ) -> Result<User, AppError> {
        let user = sqlx::query(
            "UPDATE users SET name = $1 WHERE id = $2 RETURNING id, name, email, password, avatar, role, banned, banned_until",
        )
        .bind(username)
        .bind(user_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if let sqlx::error::Error::Database(db_err) = &e {
                match db_err.constraint() {
                    Some("users_name_key" | "idx_users_name") => {
                        return AppError::UserAlreadyExist("用户名已存在".to_string());
                    }
                    Some(other) => {
                        tracing::warn!("Unexpected constraint {other}");
                    }
                    None => {}
                }
            }
            tracing::warn!("Database error {e}");
            AppError::DatabaseError(e)
        })?;

        Ok(User {
            id: UserId(user.get("id")),
            name: user.get("name"),
            email: user.get("email"),
            password: user.get("password"),
            avatar: user.get("avatar"),
            role: user.get("role"),
            banned: user.get("banned"),
            banned_until: user.get("banned_until"),
        })
    }

    pub async fn update_email(&self, user_id: &UserId, email: &str) -> Result<User, AppError> {
        let user = sqlx::query(
            "UPDATE users SET email = $1 WHERE id = $2 RETURNING id, name, email, password, avatar, role, banned, banned_until",
        )
        .bind(email)
        .bind(user_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if let sqlx::error::Error::Database(db_err) = &e {
                match db_err.constraint() {
                    Some("users_email_key" | "idx_users_email") => {
                        return AppError::UserAlreadyExist("该邮箱已被占用".to_string());
                    }
                    Some(other) => {
                        tracing::warn!("Unexpected constraint {other}");
                    }
                    None => {}
                }
            }
            tracing::warn!("Database error {e}");
            AppError::DatabaseError(e)
        })?;

        Ok(User {
            id: UserId(user.get("id")),
            name: user.get("name"),
            email: user.get("email"),
            password: user.get("password"),
            avatar: user.get("avatar"),
            role: user.get("role"),
            banned: user.get("banned"),
            banned_until: user.get("banned_until"),
        })
    }

    pub async fn update_password(
        &self,
        user_id: &UserId,
        new_password: &str,
    ) -> Result<(), AppError> {
        sqlx::query("UPDATE users SET password = $1 WHERE id = $2")
            .bind(Self::password_hash(new_password)?)
            .bind(user_id.0)
            .execute(&self.pool)
            .await
            .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        Ok(())
    }

    pub async fn update_avatar(&self, user_id: &UserId, avatar: &str) -> Result<User, AppError> {
        let user = sqlx::query(
            "UPDATE users SET avatar = $1 WHERE id = $2 RETURNING id, name, email, password, avatar, role, banned, banned_until",
        )
        .bind(avatar)
        .bind(user_id.0)
        .fetch_one(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        Ok(User {
            id: UserId(user.get("id")),
            name: user.get("name"),
            email: user.get("email"),
            password: user.get("password"),
            avatar: user.get("avatar"),
            role: user.get("role"),
            banned: user.get("banned"),
            banned_until: user.get("banned_until"),
        })
    }

    /// 标记 / 解除用户封禁。只改 banned 字段，不删数据，因此可逆。
    pub async fn set_user_banned(&self, user_id: &UserId, banned: bool) -> Result<(), AppError> {
        sqlx::query("UPDATE users SET banned = $1 WHERE id = $2")
            .bind(banned)
            .bind(user_id.0)
            .execute(&self.pool)
            .await
            .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        Ok(())
    }

    /// 设置封禁到期时间戳（毫秒）。Some(until) = 封禁至该时刻；None = 解封。
    /// 同步把旧 banned bool 列写成 until.is_some()，保持两列一致（旧列仅作历史兼容）。
    pub async fn set_user_banned_until(
        &self,
        user_id: &UserId,
        until: Option<i64>,
    ) -> Result<(), AppError> {
        sqlx::query("UPDATE users SET banned_until = $1, banned = $2 WHERE id = $3")
            .bind(until)
            .bind(until.is_some())
            .bind(user_id.0)
            .execute(&self.pool)
            .await
            .inspect_err(|e| tracing::warn!("Database error {e}"))?;

        Ok(())
    }

    pub fn password_hash(password: &str) -> Result<String, AppError> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        argon2
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(Into::into)
    }

    pub fn check_password(password: &str, stored_password: &str) -> bool {
        let result = PasswordHash::new(stored_password).and_then(|parsed_hash| {
            Argon2::default().verify_password(password.as_bytes(), &parsed_hash)
        });
        result.is_ok()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("hello")]
    #[case("Complicated@123456789")]
    fn test_password_checker(#[case] origin_password: &str) {
        assert!(UserRepository::check_password(
            origin_password,
            &UserRepository::password_hash(origin_password).unwrap()
        ));
    }
}
