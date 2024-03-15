use std::{collections::HashMap, pin::Pin};

use actix_web::error::ErrorInternalServerError;
use aws_config::BehaviorVersion;
use aws_sdk_ssm::{config::Region, Client};
use chrono::NaiveDateTime;
use futures_util::Future;
use moosicbox_database::{
    boxed,
    query::{where_eq, where_gte, FilterableQuery},
    Database, DatabaseValue, Row,
};
use moosicbox_json_utils::{database::ToValue, MissingValue, ParseError, ToValueType};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;

impl From<DatabaseError> for actix_web::Error {
    fn from(value: DatabaseError) -> Self {
        log::error!("{value:?}");
        ErrorInternalServerError(value)
    }
}

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error(transparent)]
    Db(#[from] moosicbox_database::DatabaseError),
    #[error(transparent)]
    Parse(#[from] moosicbox_json_utils::ParseError),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Connection {
    pub client_id: String,
    pub tunnel_ws_id: String,
    pub created: NaiveDateTime,
    pub updated: NaiveDateTime,
}

impl MissingValue<Connection> for &moosicbox_database::Row {}
impl ToValueType<Connection> for &Row {
    fn to_value_type(self) -> Result<Connection, ParseError> {
        Ok(Connection {
            client_id: self.to_value("client_id")?,
            tunnel_ws_id: self.to_value("tunnel_ws_id")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SignatureToken {
    pub token_hash: String,
    pub client_id: String,
    pub expires: NaiveDateTime,
    pub created: NaiveDateTime,
    pub updated: NaiveDateTime,
}

impl MissingValue<SignatureToken> for &moosicbox_database::Row {}
impl ToValueType<SignatureToken> for &Row {
    fn to_value_type(self) -> Result<SignatureToken, ParseError> {
        Ok(SignatureToken {
            token_hash: self.to_value("token_hash")?,
            client_id: self.to_value("client_id")?,
            expires: self.to_value("expires")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClientAccessToken {
    pub token_hash: String,
    pub client_id: String,
    pub expires: Option<NaiveDateTime>,
    pub created: NaiveDateTime,
    pub updated: NaiveDateTime,
}

impl MissingValue<ClientAccessToken> for &moosicbox_database::Row {}
impl ToValueType<ClientAccessToken> for &Row {
    fn to_value_type(self) -> Result<ClientAccessToken, ParseError> {
        Ok(ClientAccessToken {
            token_hash: self.to_value("token_hash")?,
            client_id: self.to_value("client_id")?,
            expires: self.to_value("expires")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MagicToken {
    pub magic_token_hash: String,
    pub client_id: String,
    pub expires: Option<NaiveDateTime>,
    pub created: NaiveDateTime,
    pub updated: NaiveDateTime,
}

impl MissingValue<MagicToken> for &moosicbox_database::Row {}
impl ToValueType<MagicToken> for &Row {
    fn to_value_type(self) -> Result<MagicToken, ParseError> {
        Ok(MagicToken {
            magic_token_hash: self.to_value("magic_token_hash")?,
            client_id: self.to_value("client_id")?,
            expires: self.to_value("expires")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

static DB: Lazy<Mutex<Option<Box<dyn Database>>>> = Lazy::new(|| Mutex::new(None));

pub async fn init() -> Result<(), DatabaseError> {
    let config = aws_config::defaults(BehaviorVersion::v2023_11_09())
        .region(Region::new("us-east-1"))
        .load()
        .await;

    let client = Client::new(&config);

    let params = match client
        .get_parameters()
        .set_with_decryption(Some(true))
        .names("moosicbox_db_name")
        .names("moosicbox_db_hostname")
        .names("moosicbox_db_password")
        .names("moosicbox_db_user")
        .send()
        .await
    {
        Ok(params) => params,
        Err(err) => panic!("Failed to get parameters {err:?}"),
    };

    let params = params.parameters.expect("Failed to get params");
    #[allow(unused)]
    let params: HashMap<&str, &str> = params
        .iter()
        .map(|param| (param.name().unwrap(), param.value().unwrap()))
        .collect();

    #[cfg(feature = "postgres")]
    {
        use std::sync::Arc;

        use moosicbox_database::sqlx::postgres::PostgresSqlxDatabase;
        use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect_with(
                PgConnectOptions::new()
                    .host(
                        params
                            .get("moosicbox_db_hostname")
                            .cloned()
                            .expect("No hostname"),
                    )
                    .database(
                        params
                            .get("moosicbox_db_name")
                            .cloned()
                            .expect("No db_name"),
                    )
                    .username(
                        params
                            .get("moosicbox_db_user")
                            .cloned()
                            .expect("No db_user"),
                    )
                    .password(
                        params
                            .get("moosicbox_db_password")
                            .cloned()
                            .expect("No db_password"),
                    ),
            )
            .await
            .map_err(|e| moosicbox_database::DatabaseError::PostgresSqlx(e.into()))?;

        DB.lock()
            .await
            .replace(Box::new(PostgresSqlxDatabase::new(Arc::new(
                tokio::sync::Mutex::new(pool),
            ))));
    }

    Ok(())
}

async fn resilient_exec<T, F>(
    exec: Box<dyn Fn() -> Pin<Box<F>> + Send + Sync>,
) -> Result<T, DatabaseError>
where
    F: Future<Output = Result<T, DatabaseError>> + Send + 'static,
{
    #[allow(unused)]
    static MAX_RETRY: u8 = 3;
    #[allow(unused)]
    let mut retries = 0;
    loop {
        match exec().await {
            Ok(value) => return Ok(value),
            Err(err) => {
                #[cfg(feature = "postgres")]
                {
                    match err {
                        DatabaseError::Db(moosicbox_database::DatabaseError::PostgresSqlx(
                            ref postgres_err,
                        )) => match postgres_err {
                            moosicbox_database::sqlx::postgres::SqlxDatabaseError::Sqlx(
                                sqlx::Error::Io(_io_err),
                            ) => {
                                if retries >= MAX_RETRY {
                                    return Err(err);
                                }
                                log::info!(
                                    "Database IO error. Attempting reconnect... {}/{MAX_RETRY}",
                                    retries + 1
                                );
                                if let Err(init_err) = init().await {
                                    log::error!("Failed to reinitialize: {init_err:?}");
                                    return Err(init_err);
                                }
                                retries += 1;
                                continue;
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                }
                return Err(err);
            }
        }
    }
}

pub async fn upsert_connection(client_id: &str, tunnel_ws_id: &str) -> Result<(), DatabaseError> {
    let client_id = client_id.to_owned();
    let tunnel_ws_id = tunnel_ws_id.to_owned();

    resilient_exec(Box::new(move || {
        let client_id = client_id.clone();
        let tunnel_ws_id = tunnel_ws_id.clone();

        Box::pin(async move {
            moosicbox_database::query::upsert("connections")
                .value("client_id", client_id.clone())
                .value("tunnel_ws_id", tunnel_ws_id.clone())
                .execute(DB.lock().await.as_mut().expect("DB not initialized"))
                .await?;

            Ok(())
        })
    }))
    .await
}

pub async fn select_connection(client_id: &str) -> Result<Option<Connection>, DatabaseError> {
    let client_id = client_id.to_owned();

    resilient_exec(Box::new(move || {
        let client_id = client_id.clone();

        Box::pin(async move {
            Ok(moosicbox_database::query::select("connections")
                .where_eq("client_id", client_id)
                .execute_first(DB.lock().await.as_mut().expect("DB not initialized"))
                .await?
                .as_ref()
                .to_value_type()?)
        })
    }))
    .await
}

pub async fn delete_connection(tunnel_ws_id: &str) -> Result<(), DatabaseError> {
    let tunnel_ws_id = tunnel_ws_id.to_owned();

    resilient_exec(Box::new(move || {
        let tunnel_ws_id = tunnel_ws_id.clone();

        Box::pin(async move {
            moosicbox_database::query::delete("connections")
                .where_eq("tunnel_ws_id", tunnel_ws_id)
                .execute(DB.lock().await.as_mut().expect("DB not initialized"))
                .await?;

            Ok(())
        })
    }))
    .await
}

pub async fn insert_client_access_token(
    client_id: &str,
    token_hash: &str,
) -> Result<(), DatabaseError> {
    let client_id = client_id.to_owned();
    let token_hash = token_hash.to_owned();

    resilient_exec(Box::new(move || {
        let client_id = client_id.clone();
        let token_hash = token_hash.clone();

        Box::pin(async move {
            moosicbox_database::query::insert("client_access_tokens")
                .value("token_hash", token_hash)
                .value("client_id", client_id)
                .value("expires", DatabaseValue::Null)
                .execute(DB.lock().await.as_mut().expect("DB not initialized"))
                .await?;

            Ok(())
        })
    }))
    .await
}

pub async fn valid_client_access_token(
    client_id: &str,
    token_hash: &str,
) -> Result<bool, DatabaseError> {
    Ok(select_client_access_token(client_id, token_hash)
        .await?
        .is_some())
}

pub async fn select_client_access_token(
    client_id: &str,
    token_hash: &str,
) -> Result<Option<ClientAccessToken>, DatabaseError> {
    let client_id = client_id.to_owned();
    let token_hash = token_hash.to_owned();

    resilient_exec(Box::new(move || {
        let client_id = client_id.clone();
        let token_hash = token_hash.clone();

        Box::pin(async move {
            Ok(moosicbox_database::query::select("client_access_tokens")
                .where_eq("client_id", client_id)
                .where_eq("token_hash", token_hash)
                .where_or(boxed!(
                    where_eq("expires", DatabaseValue::Null),
                    where_gte("expires", DatabaseValue::Now)
                ))
                .execute_first(DB.lock().await.as_mut().expect("DB not initialized"))
                .await?
                .as_ref()
                .to_value_type()?)
        })
    }))
    .await
}

pub async fn insert_magic_token(
    client_id: &str,
    magic_token_hash: &str,
) -> Result<(), DatabaseError> {
    let magic_token_hash = magic_token_hash.to_owned();
    let client_id = client_id.to_owned();

    resilient_exec(Box::new(move || {
        let magic_token_hash = magic_token_hash.clone();
        let client_id = client_id.clone();

        Box::pin(async move {
            moosicbox_database::query::insert("magic_tokens")
                .value("magic_token_hash", magic_token_hash)
                .value("client_id", client_id)
                .value("expires", DatabaseValue::Null)
                .execute(DB.lock().await.as_mut().expect("DB not initialized"))
                .await?;

            Ok(())
        })
    }))
    .await
}

pub async fn select_magic_token(token_hash: &str) -> Result<Option<MagicToken>, DatabaseError> {
    let token_hash = token_hash.to_owned();

    resilient_exec(Box::new(move || {
        let token_hash = token_hash.clone();

        Box::pin(async move {
            Ok(moosicbox_database::query::select("magic_tokens")
                .where_eq("magic_token_hash", token_hash)
                .where_or(boxed!(
                    where_eq("expires", DatabaseValue::Null),
                    where_gte("expires", DatabaseValue::Now)
                ))
                .execute_first(DB.lock().await.as_mut().expect("DB not initialized"))
                .await?
                .as_ref()
                .to_value_type()?)
        })
    }))
    .await
}

pub async fn insert_signature_token(
    client_id: &str,
    token_hash: &str,
) -> Result<(), DatabaseError> {
    let token_hash = token_hash.to_owned();
    let client_id = client_id.to_owned();

    resilient_exec(Box::new(move || {
        let token_hash = token_hash.clone();
        let client_id = client_id.clone();

        Box::pin(async move {
            moosicbox_database::query::insert("signature_tokens")
                .value("token_hash", token_hash)
                .value("client_id", client_id)
                .value(
                    "expires",
                    DatabaseValue::NowAdd("INTERVAL '14 day'".to_string()),
                )
                .execute(DB.lock().await.as_mut().expect("DB not initialized"))
                .await?;

            Ok(())
        })
    }))
    .await
}

pub async fn valid_signature_token(
    client_id: &str,
    token_hash: &str,
) -> Result<bool, DatabaseError> {
    Ok(select_signature_token(client_id, token_hash)
        .await?
        .is_some())
}

pub async fn select_signature_token(
    client_id: &str,
    token_hash: &str,
) -> Result<Option<SignatureToken>, DatabaseError> {
    let client_id = client_id.to_owned();
    let token_hash = token_hash.to_owned();

    resilient_exec(Box::new(move || {
        let client_id = client_id.clone();
        let token_hash = token_hash.clone();

        Box::pin(async move {
            Ok(moosicbox_database::query::select("signature_tokens")
                .where_eq("client_id", client_id)
                .where_eq("token_hash", token_hash)
                .where_gte("expires", DatabaseValue::Now)
                .execute_first(DB.lock().await.as_mut().expect("DB not initialized"))
                .await?
                .as_ref()
                .to_value_type()?)
        })
    }))
    .await
}

#[allow(dead_code)]
pub async fn select_signature_tokens(
    client_id: &str,
) -> Result<Vec<SignatureToken>, DatabaseError> {
    let client_id = client_id.to_owned();

    resilient_exec(Box::new(move || {
        let client_id = client_id.clone();

        Box::pin(async move {
            Ok(moosicbox_database::query::select("signature_tokens")
                .where_eq("client_id", client_id)
                .execute(DB.lock().await.as_mut().expect("DB not initialized"))
                .await?
                .to_value_type()?)
        })
    }))
    .await
}

#[allow(dead_code)]
pub async fn delete_signature_token(token_hash: &str) -> Result<(), DatabaseError> {
    let token_hash = token_hash.to_owned();

    resilient_exec(Box::new(move || {
        let token_hash = token_hash.clone();

        Box::pin(async move {
            moosicbox_database::query::delete("signature_tokens")
                .where_eq("token_hash", token_hash)
                .execute(DB.lock().await.as_mut().expect("DB not initialized"))
                .await?;

            Ok(())
        })
    }))
    .await
}