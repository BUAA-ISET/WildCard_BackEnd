mod domain;
mod error;
mod infrastructure;
mod interface;
mod state;

use crate::infrastructure::room::RoomRepository;
use crate::infrastructure::user::UserRepository;
use crate::state::{GlobalState, JwtSecret};
use axum::Router;
use dotenv::dotenv;
use sqlx::PgPool;
use std::{env, sync::Arc};
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

    let secret_key = env::var("SECRET_KEY").expect("SECRET_KEY must be set for generating JWT");
    info!("Load environment SECRET_KEY={secret_key}");

    let listen_address = env::var("LISTEN_ADDRESS").unwrap_or("0.0.0.0:3000".to_string());
    info!("Load environment LISTEN_ADDRESS={listen_address}");

    // Connect to postgres database.
    let pool = PgPool::connect_lazy(&database_url).expect("Failed to connect to the database");

    let state = GlobalState {
        jwt_secret: JwtSecret(secret_key.into_bytes()),
        user_repo: Arc::new(UserRepository { pool: pool.clone() }),
        room_repo: Arc::new(RoomRepository::default()),
    };

    let app = build_route(state);

    let listener = tokio::net::TcpListener::bind(&listen_address)
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}

fn build_route(state: GlobalState) -> Router {
    use crate::interface::*;
    use axum::routing::*;

    Router::new()
        // User api
        .route("/api/user/register", post(user::register_handler))
        .route("/api/user/find", get(user::find_handler))
        .route("/api/user/login", post(user::login_handler))
        .route("/api/user/logout", any(user::logout_handler))
        .route("/api/user/me", get(user::me))
        // Room api
        .route("/api/room/create", post(room::create_handler))
        .route("/api/room/find", get(room::find_handler))
        .route("/api/room/delete", post(room::delete_handler))
        .route("/api/room/replace_owner", post(room::replace_owner_handler))
        .route("/api/room/rule/get", get(room::get_rule_handler))
        .route("/api/room/enter", any(room::enter_handler))
        .layer(TraceLayer::new_for_http()) // Add a TraceLayer to automatically create and enter spans
        .with_state(state)
}
