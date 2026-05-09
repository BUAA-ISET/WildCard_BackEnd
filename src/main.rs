mod domain;
mod error;
mod infrastructure;
mod interface;
mod state;

use crate::infrastructure::user::UserRepository;
use crate::interface::{room, rule, user};
use crate::state::{GlobalState, JwtSecret};

use axum::{
    Router,
    http::{HeaderName, HeaderValue, Method},
    routing::{get, post},
};
use dotenv::dotenv;
use sqlx::PgPool;
use std::{env, sync::Arc};
use tokio::sync::RwLock;
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    trace::TraceLayer,
};
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

    let secret_key = env::var("SECRET_KEY").expect("SECRET_KEY must be set for generating JWT");
    info!("Load environment SECRET_KEY={secret_key}");

    let listen_address = env::var("LISTEN_ADDRESS").unwrap_or("0.0.0.0:3000".to_string());
    info!("Load environment LISTEN_ADDRESS={listen_address}");

    // Connect to postgres database.
    let pool = PgPool::connect_lazy(&database_url).expect("Failed to connect to the database");

    let state = GlobalState {
        jwt_secret: JwtSecret(secret_key.into_bytes()),
        user: Arc::new(UserRepository { pool: pool.clone() }),
        verification_codes: Arc::new(RwLock::new(Default::default())),

        games: Arc::new(RwLock::new(Default::default())),
        rules: rule::build_rule_store(),
        rooms: room::build_room_store(),
    };

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list([
            HeaderValue::from_static("http://localhost:5173"),
            HeaderValue::from_static("http://127.0.0.1:5173"),
        ]))
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::OPTIONS])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            HeaderName::from_static("x-player-id"),
            HeaderName::from_static("x-player-name"),
            HeaderName::from_static("x-player-avatar"),
        ])
        .allow_credentials(true);

    let app = Router::new()
        .route("/api/user/register", post(user::register))
        .route("/api/user/send-code", post(user::send_verification_code))
        .route("/api/user/find", get(user::find))
        .route("/api/user/login", post(user::login))
        .route("/api/user/logout", post(user::logout).get(user::logout))
        .route("/api/user/current", get(user::current))
        .route("/api/user/me", get(user::current))
        .route(
            "/api/user/username",
            post(user::update_username).put(user::update_username),
        )
        .route(
            "/api/user/password",
            post(user::update_password).put(user::update_password),
        )
        .route("/api/rules/drafts", post(rule::save_draft))
        .route(
            "/api/rules/drafts/{draft_id}",
            get(rule::get_draft).put(rule::update_draft),
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
        .layer(cors)
        .layer(TraceLayer::new_for_http()) // Add a TraceLayer to automatically create and enter spans
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&listen_address)
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
