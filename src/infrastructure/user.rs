use argon2::Argon2;
use password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng};
use sqlx::PgPool;

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
        sqlx::query!(
            "INSERT INTO users (id, name, email, password) VALUES ($1, $2, $3, $4)",
            user.id.0,
            user.name,
            user.email,
            Self::password_hash(&user.password)?
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            if let sqlx::error::Error::Database(db_err) = &e {
                match db_err.constraint() {
                    Some("users_name_key") => {
                        return AppError::UserAlreadyExist(format!("用户名 {} 已存在", user.name));
                    }
                    Some("users_email_key") => {
                        return AppError::UserAlreadyExist(format!("邮箱 {} 已存在", user.email));
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
        let user = sqlx::query!(
            "SELECT id, name, email, password FROM users WHERE users.name = $1",
            name
        )
        .fetch_optional(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?
        .map(|user| User {
            id: UserId(user.id),
            name: user.name,
            email: user.email,
            password: user.password,
        });

        Ok(user)
    }

    pub async fn find_by_id(&self, user_id: &UserId) -> Result<Option<User>, AppError> {
        let user = sqlx::query!(
            "SELECT id, name, email, password FROM users WHERE users.id = $1",
            user_id.0
        )
        .fetch_optional(&self.pool)
        .await
        .inspect_err(|e| tracing::warn!("Database error {e}"))?
        .map(|user| User {
            id: UserId(user.id),
            name: user.name,
            email: user.email,
            password: user.password,
        });

        Ok(user)
    }

    pub fn password_hash(password: &str) -> Result<String, AppError> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        argon2
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|e| e.into())
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
