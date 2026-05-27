use sqlx::PgPool;

pub struct RoomRepository {
    pub pg_pool: PgPool,
}
