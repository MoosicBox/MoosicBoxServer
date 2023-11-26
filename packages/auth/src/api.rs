use actix_web::{
    error::{ErrorInternalServerError, ErrorUnauthorized},
    route,
    web::{self, Json},
    Result,
};
use moosicbox_core::app::AppState;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{create_magic_token, get_credentials_from_magic_token};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MagicTokenQuery {
    magic_token: String,
}

#[route("/auth/magic-token", method = "GET")]
pub async fn get_magic_token_endpoint(
    query: web::Query<MagicTokenQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    if let Some((client_id, access_token)) = get_credentials_from_magic_token(
        &data
            .db
            .clone()
            .ok_or(ErrorInternalServerError("No DB set"))?
            .library
            .as_ref()
            .lock()
            .unwrap(),
        &query.magic_token,
    )
    .map_err(|e| ErrorInternalServerError(format!("Failed to get magic token: {e:?}")))?
    {
        Ok(Json(
            json!({"client_id": client_id, "access_token": access_token}),
        ))
    } else {
        Err(ErrorUnauthorized("Unauthorized"))
    }
}

#[route("/auth/magic-token", method = "POST")]
pub async fn magic_token_endpoint(data: web::Data<AppState>) -> Result<Json<Value>> {
    let token = create_magic_token(
        &data
            .db
            .clone()
            .ok_or(ErrorInternalServerError("No DB set"))?
            .library
            .as_ref()
            .lock()
            .unwrap(),
    )
    .map_err(|e| ErrorInternalServerError(format!("Failed to create magic token: {e:?}")))?;

    Ok(Json(json!({"token": token})))
}
