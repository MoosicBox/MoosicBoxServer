use moosicbox_core::sqlite::db::{select, upsert, DbError, SqliteValue};
use rusqlite::Connection;

pub mod models;

use crate::db::models::QobuzConfig;

#[allow(clippy::too_many_arguments)]
pub fn create_qobuz_config(
    db: &Connection,
    access_token: &str,
    refresh_token: &str,
    client_name: &str,
    expires_in: u32,
    scope: &str,
    token_type: &str,
    user: &str,
    user_id: u32,
) -> Result<(), DbError> {
    upsert::<QobuzConfig>(
        db,
        "qobuz_config",
        vec![(
            "refresh_token",
            SqliteValue::String(refresh_token.to_string()),
        )],
        vec![
            (
                "access_token",
                SqliteValue::String(access_token.to_string()),
            ),
            (
                "refresh_token",
                SqliteValue::String(refresh_token.to_string()),
            ),
            ("client_name", SqliteValue::String(client_name.to_string())),
            ("expires_in", SqliteValue::Number(expires_in as i64)),
            ("scope", SqliteValue::String(scope.to_string())),
            ("token_type", SqliteValue::String(token_type.to_string())),
            ("user", SqliteValue::String(user.to_string())),
            ("user_id", SqliteValue::Number(user_id as i64)),
        ],
    )?;

    Ok(())
}

pub fn get_qobuz_config(db: &Connection) -> Result<Option<QobuzConfig>, DbError> {
    let mut configs = select::<QobuzConfig>(db, "qobuz_config", &vec![], &["*"])?
        .into_iter()
        .collect::<Vec<_>>();

    if configs.is_empty() {
        return Err(DbError::Unknown);
    }

    configs.sort_by(|a, b| a.issued_at.cmp(&b.issued_at));

    Ok(configs.first().cloned())
}

pub fn get_qobuz_access_token(db: &Connection) -> Result<Option<String>, DbError> {
    Ok(get_qobuz_config(db)?.map(|c| c.access_token.clone()))
}
