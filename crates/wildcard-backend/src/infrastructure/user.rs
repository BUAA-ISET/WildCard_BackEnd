use crate::{
    domain::{
        email::MailAddress,
        user::{User, UserId},
    },
    error::AppError,
    state::UploadDir,
};
use argon2::Argon2;
use deadpool_redis::Pool as RedisPool;
use image::ImageFormat;
use password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng};
use rand::Rng;
use redis::AsyncTypedCommands;
use sqlx::PgPool;
use std::io::Cursor;
use tracing::{error, warn};
use uuid::Uuid;

#[derive(Debug)]
pub struct UserRepository {
    pub pg_pool: PgPool,
    pub redis_pool: RedisPool,
}

#[derive(Debug)]
struct UserRecord {
    id: Uuid,
    name: String,
    email: String,
    avatar: String,
    password: String,
}

impl TryFrom<UserRecord> for User {
    type Error = AppError;

    fn try_from(
        UserRecord {
            id,
            name,
            email,
            avatar,
            password,
        }: UserRecord,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            id: UserId(id),
            name,
            email: email
                .parse::<MailAddress>()
                .inspect_err(|e| warn!("Email store in database is in wrong format: {e}"))
                .map_err(|e| sqlx::Error::Encode(Box::new(e)))?,
            avatar,
            password,
        })
    }
}

impl UserRepository {
    const UNIQUE_NAME_CONSTRAINT: &str = "users_name_key";
    const UNIQUE_EMAIL_CONSTRAINT: &str = "users_email_key";

    pub async fn register(&self, user: User) -> Result<(), AppError> {
        sqlx::query!(
            "INSERT INTO users (id, name, email, password) VALUES ($1, $2, $3, $4)",
            user.id.0,
            user.name,
            user.email.as_ref() as &str,
            Self::password_hash(&user.password)?
        )
        .execute(&self.pg_pool)
        .await
        .map_err(|e| {
            if let sqlx::error::Error::Database(db_err) = &e {
                match db_err.constraint() {
                    Some("idx_users_name") => {
                        return AppError::UserAlreadyExist("用户名已存在".to_string());
                    }
                    Some("idx_users_email") => {
                        return AppError::UserAlreadyExist("该邮箱已注册".to_string());
                    }
                    Some(other) => {
                        warn!("Unexpected constraint {other}");
                    }
                    None => {}
                }
            }
            warn!("Database error {e}");
            AppError::DatabaseSql(e)
        })?;
        Ok(())
    }

    fn process_user_query(
        record: Result<Option<UserRecord>, sqlx::Error>,
    ) -> Result<Option<User>, AppError> {
        record
            .inspect_err(|e| warn!("Database error {e}"))?
            .map(User::try_from)
            .transpose()
    }

    pub async fn find_by_name(&self, name: &str) -> Result<Option<User>, AppError> {
        let record = sqlx::query_as!(
            UserRecord,
            "SELECT id, name, email, avatar, password FROM users WHERE users.name = $1",
            name
        )
        .fetch_optional(&self.pg_pool)
        .await;
        Self::process_user_query(record)
    }

    pub async fn find_by_id(&self, user_id: &UserId) -> Result<Option<User>, AppError> {
        let record = sqlx::query_as!(
            UserRecord,
            "SELECT id, name, email, avatar, password FROM users WHERE users.id = $1",
            user_id.0
        )
        .fetch_optional(&self.pg_pool)
        .await;
        Self::process_user_query(record)
    }

    pub async fn find_by_email(&self, email: &MailAddress) -> Result<Option<User>, AppError> {
        let record = sqlx::query_as!(
            UserRecord,
            "SELECT id, name, email, avatar, password FROM users WHERE users.email = $1",
            email.as_ref() as &str
        )
        .fetch_optional(&self.pg_pool)
        .await;
        Self::process_user_query(record)
    }

    pub async fn update_user_name(
        &self,
        user_id: &UserId,
        username: &str,
    ) -> Result<User, AppError> {
        let record = sqlx::query_as!(
            UserRecord,
            "UPDATE users SET name = $1 WHERE id = $2 RETURNING id, name, email, avatar, password",
            username,
            user_id.0
        )
        .fetch_one(&self.pg_pool)
        .await
        .map_err(|e| {
            if let sqlx::error::Error::Database(db_err) = &e {
                match db_err.constraint() {
                    Some(Self::UNIQUE_NAME_CONSTRAINT) => {
                        return AppError::UserAlreadyExist("用户名已存在".to_string());
                    }
                    Some(other) => {
                        warn!("Unexpected constraint {other}");
                    }
                    None => {}
                }
            }
            warn!("Database error {e}");
            AppError::DatabaseSql(e)
        })?;

        User::try_from(record)
    }

    pub async fn update_email(
        &self,
        user_id: &UserId,
        email: &MailAddress,
    ) -> Result<User, AppError> {
        let record = sqlx::query_as!(
            UserRecord,
            "UPDATE users SET email = $1 WHERE id = $2 RETURNING id, name, email, avatar, password",
            email.as_ref() as &str,
            user_id.0,
        )
        .fetch_one(&self.pg_pool)
        .await
        .map_err(|e| {
            if let sqlx::error::Error::Database(db_err) = &e {
                match db_err.constraint() {
                    Some(Self::UNIQUE_EMAIL_CONSTRAINT) => {
                        return AppError::UserAlreadyExist("该邮箱已被占用".to_string());
                    }
                    Some(other) => {
                        warn!("Unexpected constraint {other}");
                    }
                    None => {}
                }
            }
            warn!("Database error {e}");
            AppError::DatabaseSql(e)
        })?;

        User::try_from(record)
    }

    pub async fn update_password(
        &self,
        user_id: &UserId,
        new_password: &str,
    ) -> Result<(), AppError> {
        sqlx::query!(
            "UPDATE users SET password = $1 WHERE id = $2",
            Self::password_hash(new_password)?,
            user_id.0
        )
        .execute(&self.pg_pool)
        .await
        .inspect_err(|e| warn!("Database error {e}"))?;

        Ok(())
    }

    pub async fn update_avatar(
        &self,
        upload_dir: UploadDir,
        user_id: &UserId,
        img_data: Vec<u8>,
        mime: &str,
    ) -> Result<User, AppError> {
        let image_format = ImageFormat::from_mime_type(mime)
            .ok_or_else(|| AppError::InvalidInput("不支持的图片格式".to_string()))?;

        // Filename and extension
        let filename = format!("{}.webp", Uuid::new_v4());
        let avatars_dir = upload_dir.join("avatar");
        tokio::fs::create_dir_all(&avatars_dir).await?;
        let avatars_file = avatars_dir.join(&filename);

        // Process image
        let img_data = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, AppError> {
            let img = image::load_from_memory_with_format(&img_data, image_format)
                .map_err(|e| AppError::InvalidInput(format!("图片解析失败：{e}")))?;
            let mut buffer = Cursor::new(Vec::new());
            img.thumbnail(200, 200)
                .write_to(&mut buffer, ImageFormat::WebP)
                .inspect_err(|e| error!("Failed to process avatar: {e}"))
                .map_err(|e| AppError::InvalidInput(format!("图片处理失败：{e}")))?;

            Ok(buffer.into_inner())
        })
        .await??;

        // Write to db
        let tx = self.pg_pool.begin().await?;
        let record = sqlx::query!(
            r#"
            WITH old_data AS (
                SELECT avatar AS old_avatar FROM users WHERE id = $2
            )
            UPDATE users SET avatar = $1 FROM old_data
            RETURNING id, name, email, avatar, password, old_avatar
            "#,
            filename,
            user_id.0,
        )
        .fetch_one(&self.pg_pool)
        .await?;
        let id = record.id;
        let name = record.name;
        let email = record.email;
        let avatar = record.avatar; // filename
        let password = record.password;
        let old_avatar = record.old_avatar; // filename

        // Write file
        tokio::fs::write(&avatars_file, &img_data).await?;

        // Delete old image
        if !old_avatar.is_empty() {
            let old_file = avatars_dir.join(old_avatar);
            tokio::fs::remove_file(&old_file)
                .await
                .map_err(AppError::from)
                .inspect_err(|e| error!("图片删除失败：{e}"))
                .ok();
            // 即使失败也不影响用户体验
        }

        tx.commit().await?;

        User::try_from(UserRecord {
            id,
            name,
            email,
            avatar,
            password,
        })
    }

    pub async fn generate_code(&self, email: &MailAddress) -> Result<String, AppError> {
        let value: u32 = rand::rng().random_range(0..1_000_000);
        let code = format!("{value:06}");
        let mut con = self.redis_pool.get().await?;
        con.set_ex(format!("email_code:{email}"), &code, 10 * 60)
            .await?;
        Ok(code)
    }

    pub async fn verify_code(&self, email: &MailAddress, code: &str) -> Result<(), AppError> {
        let mut con = self.redis_pool.get().await?;
        let stored_code: Option<String> = con.get(format!("email_code:{email}")).await?;
        let result = match stored_code {
            Some(stored_code) => stored_code == code,
            None => false,
        };
        if !result {
            return Err(AppError::InvalidInput("验证码错误".to_string()));
        }
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
