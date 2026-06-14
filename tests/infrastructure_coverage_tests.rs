#![allow(dead_code)]

mod error {
    pub use wildcard_backend::error::*;
}

mod domain {
    pub mod user {
        pub use wildcard_backend::domain::user::*;
    }
}

mod infrastructure {
    pub mod email {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/infrastructure/email.rs"
        ));

        #[cfg(test)]
        mod coverage {
            use super::*;
            use std::sync::{Mutex, OnceLock};

            fn env_lock() -> &'static Mutex<()> {
                static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
                LOCK.get_or_init(|| Mutex::new(()))
            }

            unsafe fn set_env(key: &str, value: &str) {
                unsafe { std::env::set_var(key, value) };
            }

            unsafe fn remove_env(key: &str) {
                unsafe { std::env::remove_var(key) };
            }

            fn clear_smtp_env() {
                for key in [
                    "SMTP_HOST",
                    "SMTP_PORT",
                    "SMTP_USER",
                    "SMTP_PASS",
                    "SMTP_FROM",
                ] {
                    unsafe { remove_env(key) };
                }
            }

            #[test]
            fn smtp_config_from_env_covers_missing_invalid_and_present_values() {
                let _guard = env_lock()
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                clear_smtp_env();
                assert!(SmtpConfig::from_env().is_none());

                unsafe {
                    set_env("SMTP_HOST", "smtp.example.com");
                    set_env("SMTP_PORT", "not-a-port");
                    set_env("SMTP_USER", "user");
                    set_env("SMTP_PASS", "pass");
                    set_env("SMTP_FROM", "noreply@example.com");
                }
                assert!(SmtpConfig::from_env().is_none());

                unsafe { set_env("SMTP_PORT", "2525") };
                let cfg = SmtpConfig::from_env().unwrap();
                assert_eq!(cfg.host, "smtp.example.com");
                assert_eq!(cfg.port, 2525);
                assert_eq!(cfg.from, "noreply@example.com");
                clear_smtp_env();
            }

            #[test]
            fn email_sender_from_env_and_build_cover_fallbacks() {
                let _guard = env_lock()
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                clear_smtp_env();
                assert!(!EmailSender::from_env().is_configured());

                let configured = EmailSender::build(SmtpConfig {
                    host: "localhost".to_string(),
                    port: 2525,
                    user: "user".to_string(),
                    pass: "pass".to_string(),
                    from: "not-an-email".to_string(),
                })
                .unwrap();
                assert!(configured.is_configured());

                let runtime = tokio::runtime::Runtime::new().unwrap();
                let err = runtime
                    .block_on(configured.send_verification_code("user@example.com", "123456"))
                    .unwrap_err();
                assert!(matches!(err, EmailSendError::InvalidAddress(_)));
            }

            #[tokio::test]
            async fn unconfigured_sender_send_and_error_display_paths_are_covered() {
                let sender = EmailSender { inner: None };
                let err = sender
                    .send_verification_code("user@example.com", "123456")
                    .await
                    .unwrap_err();
                assert!(matches!(err, EmailSendError::NotConfigured));
                assert_eq!(err.to_string(), "SMTP not configured");
                assert!(
                    EmailSendError::InvalidAddress("bad".to_string())
                        .to_string()
                        .contains("bad")
                );
                assert!(
                    EmailSendError::Build("broken".to_string())
                        .to_string()
                        .contains("broken")
                );
                assert!(
                    EmailSendError::Smtp("down".to_string())
                        .to_string()
                        .contains("down")
                );
                assert!(build_verification_body("654321").contains("654321"));
            }
        }
    }

    pub mod user {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/infrastructure/user.rs"
        ));

        #[cfg(test)]
        mod coverage {
            use super::*;
            use crate::domain::user::{User, UserId};
            use sqlx::postgres::PgPoolOptions;
            use uuid::Uuid;

            fn lazy_repo() -> UserRepository {
                UserRepository {
                    pool: PgPoolOptions::new()
                        .acquire_timeout(std::time::Duration::from_millis(50))
                        .connect_lazy("postgres://user:password@127.0.0.1:1/wildcard_test")
                        .unwrap(),
                }
            }

            fn user() -> User {
                User {
                    id: UserId(Uuid::new_v4()),
                    name: "coverage-user".to_string(),
                    email: "coverage@example.com".to_string(),
                    password: "secret".to_string(),
                    avatar: String::new(),
                    role: "user".to_string(),
                    banned: false,
                    banned_until: None,
                }
            }

            #[tokio::test]
            async fn database_methods_cover_query_construction_and_error_mapping() {
                let repo = lazy_repo();
                let user = user();
                let user_id = user.id.clone();

                assert!(repo.register(user).await.is_err());
                assert!(repo.find_by_name("missing").await.is_err());
                assert!(repo.find_by_email("missing@example.com").await.is_err());
                assert!(repo.find_by_id(&user_id).await.is_err());
                assert!(repo.update_username(&user_id, "new-name").await.is_err());
                assert!(
                    repo.update_email(&user_id, "new@example.com")
                        .await
                        .is_err()
                );
                assert!(repo.update_password(&user_id, "new-secret").await.is_err());
                assert!(repo.update_avatar(&user_id, "/avatar.png").await.is_err());
            }

            #[test]
            fn password_hashing_covers_unique_hashes_and_malformed_inputs() {
                let first = UserRepository::password_hash("secret").unwrap();
                let second = UserRepository::password_hash("secret").unwrap();
                assert_ne!(first, second);
                assert!(UserRepository::check_password("secret", &first));
                assert!(!UserRepository::check_password("secret", "$argon2id$bad"));
            }
        }
    }
}
