use crate::{domain::email::MailAddress, error::AppError};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::Mailbox,
    transport::smtp::authentication::Credentials,
};
use tracing::warn;

#[derive(Clone)]
pub struct EmailSender {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

#[derive(Debug)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from: Mailbox,
}

impl EmailSender {
    pub fn build(config: SmtpConfig) -> Result<Self, lettre::transport::smtp::Error> {
        let creds = Credentials::new(config.username, config.password);
        let transport = AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)?
            .port(config.port)
            .credentials(creds)
            .build();
        Ok(Self {
            transport,
            from: config.from,
        })
    }

    pub async fn send_verification_code(
        &self,
        to: MailAddress,
        code: String,
    ) -> Result<(), AppError> {
        let body = format!(
            "您的 WildCard 验证码是 {code}，5 分钟内有效。\n\n如果这不是您本人的操作，请忽略此邮件。\n\n— WildCard"
        );
        let msg = Message::builder()
            .from(self.from.clone())
            .to(Mailbox {
                name: None,
                email: to.into(),
            })
            .subject("WildCard 验证码")
            .body(body)
            .inspect_err(|e| warn!("Failed to build email message: {e}"))
            .map_err(|_| AppError::Email("邮件构建失败".to_string()))?;
        self.transport
            .send(msg)
            .await
            .inspect_err(|e| warn!("SMTP transport error: {e}"))
            .map_err(|_| AppError::Email("邮件发送失败".to_string()))?;
        Ok(())
    }
}
