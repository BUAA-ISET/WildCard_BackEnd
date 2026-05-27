pub mod domain;
pub mod error;
pub mod infrastructure;
pub mod interface;
pub mod state;

use crate::infrastructure::email::{EmailSender, SmtpConfig};
use crate::infrastructure::room::RoomRepository;
use crate::infrastructure::rule::RuleRepository;
use crate::infrastructure::user::UserRepository;
use crate::state::{GlobalState, JwtSecret, UploadDir};
use axum::{
    Router,
    routing::{get, post},
};
use deadpool_redis::Config as RedisConfig;
use dotenv::dotenv;
use sqlx::PgPool;
use std::{env, sync::Arc};
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    let load_dotenv_result = dotenv();

    // Initialize tracing + log bridging
    tracing_subscriber::fmt()
        // This allows you to use, e.g., `RUST_LOG=info` or `RUST_LOG=debug`
        // when running the app to set log levels.
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .or_else(|_| EnvFilter::try_new("wildcard-backend:error,tower_http=trace"))
                .unwrap(),
        )
        .init();

    match load_dotenv_result {
        Ok(path) => info!("Load \".env\" file ({path:?}) success."),
        Err(e) => warn!("Load \".env\" file failed: {e}."),
    }

    // Read the database path from environment variables.
    //
    // And same for the secret key that is being used to sign the JWT.
    //
    // `TcpListener` will listen to the address and port in `LISTEN_ADDRESS` variable.
    // If this is not set, the default value will be set to "0.0.0.0:3000".
    //
    // You can set them in bash like so:
    // ```bash
    // export DATABASE_URL="postgres://username:password@host:port/database_name"
    // export SECRET_KEY="secret-key"
    // ```
    //
    // Or you should set them in `.env` file.
    //
    // The program will panic if necessary environment variables are not set.
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    info!("Load environment DATABASE_URL={database_url}");

    let redis_url = env::var("REDIS_URL").expect("REDIS_URL must be set");
    info!("Load environment REDIS_URL={redis_url}");

    let secret_key = env::var("SECRET_KEY").expect("SECRET_KEY must be set for generating JWT");
    info!("Load environment SECRET_KEY={secret_key}");

    let listen_address = env::var("LISTEN_ADDRESS").unwrap_or("0.0.0.0:3000".to_string());
    info!("Load environment LISTEN_ADDRESS={listen_address}");

    let upload_dir = env::var("UPLOAD_DIR").unwrap_or("/uploads".to_string());
    info!("Load environment UPLOAD_DIR={upload_dir}");

    let smtp_config = SmtpConfig {
        host: env::var("SMTP_HOST").expect("SMTP_HOST must be set"),
        port: env::var("SMTP_PORT")
            .expect("SMTP_PORT must be set")
            .trim()
            .parse()
            .expect("SMTP_PORT can not be parsed"),
        username: env::var("SMTP_USER").expect("SMTP_USER must be set"),
        password: env::var("SMTP_PASS").expect("SMTP_PASS must be set"),
        from: env::var("SMTP_FROM")
            .expect("SMTP_FROM must be set")
            .parse()
            .expect("SMTP_FROM can not be parsed"),
    };
    info!(
        "Load SMTP environment: smtp={}:{}, username={}@{}, from={}",
        smtp_config.host,
        smtp_config.port,
        smtp_config.username,
        smtp_config.password,
        smtp_config.from
    );

    let email_sender = EmailSender::build(smtp_config).expect("EmailSender build fail");

    // Connect to postgres database.
    let pg_pool = PgPool::connect_lazy(&database_url).expect("Failed to connect to the database");
    let redis_pool = RedisConfig::from_url(&redis_url)
        .create_pool(Some(deadpool_redis::Runtime::Tokio1))
        .expect("无法创建连接池");

    let state = GlobalState {
        jwt_secret: JwtSecret(secret_key.into_bytes()),
        user: Arc::new(UserRepository {
            pg_pool: pg_pool.to_owned(),
            redis_pool,
        }),
        games: Arc::new(RwLock::new(Default::default())),
        rules: Arc::new(RuleRepository {
            pg_pool: pg_pool.to_owned(),
        }),
        rooms: Arc::new(RoomRepository {
            pg_pool: pg_pool.to_owned(),
        }),
        email: Arc::new(email_sender),
        upload_dir: UploadDir(upload_dir.into()),
    };

    let app = create_route(state);

    let listener = tokio::net::TcpListener::bind(&listen_address)
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}

fn create_route(state: GlobalState) -> axum::Router {
    use crate::interface::*;

    Router::new()
        .route("/api/user/register", post(user::register))
        .route("/api/user/send-code", post(user::verification_code))
        .route("/api/user/password-reset", post(user::password_reset))
        .route("/api/user/find", get(user::find))
        .route("/api/user/login", post(user::login))
        .route("/api/user/logout", post(user::logout))
        .route("/api/user/current", get(user::current))
        .route("/api/user/me", get(user::current))
        .route(
            "/api/user/username",
            post(user::update_user_name).put(user::update_user_name),
        )
        .route(
            "/api/user/password",
            post(user::update_password).put(user::update_password),
        )
        .route(
            "/api/user/email",
            post(user::update_email).put(user::update_email),
        )
        .route("/api/user/avatar", post(user::update_avatar))
        .route(
            "/api/rules/drafts",
            get(rule::list_drafts).post(rule::save_draft),
        )
        .route(
            "/api/rules/drafts/{draft_id}",
            get(rule::get_draft)
                .put(rule::update_draft)
                .delete(rule::delete_draft),
        )
        .route(
            "/api/rules/drafts/{draft_id}/publish",
            post(rule::publish_draft),
        )
        .route("/api/room/rules", get(rule::rule_options))
        .route("/api/room/create", post(room::create_room))
        .route("/api/room/join", post(room::join_room))
        .route("/api/room/check-password", get(room::check_password))
        .route("/api/room/current", get(room::current_room))
        .route("/api/room/current/ready", post(room::set_ready))
        .route("/api/room/current/start", post(room::start_game))
        .route("/api/room/leave", post(room::leave_room))
        .route("/api/room/rule/get", get(room::get_room_rule))
        .route("/api/games/current", get(room::current_game))
        .route("/api/games/{sessionId}", get(room::get_game))
        .route(
            "/api/games/{sessionId}/actions/{actionId}/play-cards",
            post(room::play_cards),
        )
        .route(
            "/api/games/{sessionId}/actions/{actionId}/skip",
            post(room::skip_action),
        )
        .route(
            "/api/games/{sessionId}/actions/{actionId}/choose",
            post(room::choose_action),
        )
        .layer(TraceLayer::new_for_http()) // Add a TraceLayer to automatically create and enter spans
        .with_state(state)
}
