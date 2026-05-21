use std::env;

use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    transport::smtp::authentication::Credentials,
};
use tracing::{info, warn};

#[derive(Clone)]
pub struct EmailSender {
    inner: Option<EmailSenderInner>,
}

#[derive(Clone)]
struct EmailSenderInner {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: String,
}

#[derive(Debug)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub pass: String,
    pub from: String,
}

impl SmtpConfig {
    pub fn from_env() -> Option<Self> {
        let host = env::var("SMTP_HOST")
            .ok()
            .filter(|s| !s.trim().is_empty())?;
        let port = env::var("SMTP_PORT").ok()?.trim().parse::<u16>().ok()?;
        let user = env::var("SMTP_USER")
            .ok()
            .filter(|s| !s.trim().is_empty())?;
        let pass = env::var("SMTP_PASS")
            .ok()
            .filter(|s| !s.trim().is_empty())?;
        let from = env::var("SMTP_FROM")
            .ok()
            .filter(|s| !s.trim().is_empty())?;
        Some(Self {
            host,
            port,
            user,
            pass,
            from,
        })
    }
}

impl EmailSender {
    pub fn from_env() -> Self {
        match SmtpConfig::from_env() {
            Some(cfg) => match Self::build(cfg) {
                Ok(sender) => sender,
                Err(e) => {
                    warn!("Failed to build SMTP transport: {e}; falling back to debugCode mode");
                    Self { inner: None }
                }
            },
            None => {
                warn!(
                    "SMTP env vars missing or incomplete; verification codes will be returned via debugCode"
                );
                Self { inner: None }
            }
        }
    }

    fn build(cfg: SmtpConfig) -> Result<Self, lettre::transport::smtp::Error> {
        let creds = Credentials::new(cfg.user, cfg.pass);
        let transport = AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.host)?
            .port(cfg.port)
            .credentials(creds)
            .build();
        info!(
            "SMTP configured: host={} port={} from={}",
            cfg.host, cfg.port, cfg.from
        );
        Ok(Self {
            inner: Some(EmailSenderInner {
                transport,
                from: cfg.from,
            }),
        })
    }

    pub fn is_configured(&self) -> bool {
        self.inner.is_some()
    }

    pub async fn send_verification_code(&self, to: &str, code: &str) -> Result<(), EmailSendError> {
        let inner = self.inner.as_ref().ok_or(EmailSendError::NotConfigured)?;
        let body = build_verification_body(code);
        let from = inner
            .from
            .parse()
            .map_err(|e: lettre::address::AddressError| {
                EmailSendError::InvalidAddress(e.to_string())
            })?;
        let to_addr = to.parse().map_err(|e: lettre::address::AddressError| {
            EmailSendError::InvalidAddress(e.to_string())
        })?;
        let msg = Message::builder()
            .from(from)
            .to(to_addr)
            .subject("WildCard 验证码")
            .body(body)
            .map_err(|e| EmailSendError::Build(e.to_string()))?;
        inner
            .transport
            .send(msg)
            .await
            .map_err(|e| EmailSendError::Smtp(e.to_string()))?;
        Ok(())
    }
}

pub(crate) fn build_verification_body(code: &str) -> String {
    format!(
        "您的 WildCard 验证码是 {code}，5 分钟内有效。\n\n如果这不是您本人的操作，请忽略此邮件。\n\n— WildCard"
    )
}

#[derive(Debug, thiserror::Error)]
pub enum EmailSendError {
    #[error("SMTP not configured")]
    NotConfigured,
    #[error("invalid email address: {0}")]
    InvalidAddress(String),
    #[error("failed to build email message: {0}")]
    Build(String),
    #[error("SMTP transport error: {0}")]
    Smtp(String),
}

#[cfg(test)]
mod tests {
    use super::{EmailSender, build_verification_body};

    #[test]
    fn unconfigured_sender_reports_not_configured() {
        let sender = EmailSender { inner: None };
        assert!(!sender.is_configured());
    }

    #[test]
    fn verification_body_contains_code_and_validity() {
        let body = build_verification_body("123456");
        assert!(body.contains("123456"));
        assert!(body.contains("5 分钟"));
    }
}
