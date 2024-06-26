use std::str::FromStr;

use actix_web::{
    delete,
    error::{ErrorBadRequest, ErrorInternalServerError},
    get, post,
    web::{self, Json},
    Result,
};
use moosicbox_auth::NonTunnelRequestAuthorized;
use moosicbox_core::app::AppState;
use serde::Deserialize;
use serde_json::Value;

use crate::{disable_scan_origin, enable_scan_origin, get_scan_origins, scan, ScanOrigin};

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanQuery {
    origins: Option<String>,
}

#[post("/run-scan")]
pub async fn run_scan_endpoint(
    query: web::Query<ScanQuery>,
    data: web::Data<AppState>,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    let origins = query
        .origins
        .as_ref()
        .map(|origins| {
            origins
                .split(',')
                .map(|s| s.trim())
                .map(|s| {
                    ScanOrigin::from_str(s)
                        .map_err(|_e| ErrorBadRequest(format!("Invalid ScanOrigin value: {s}")))
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?;

    scan(data.database.clone(), origins)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to scan: {e:?}")))?;

    Ok(Json(serde_json::json!({"success": true})))
}

#[cfg(feature = "local")]
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanPathQuery {
    path: String,
}

#[cfg(feature = "local")]
#[post("/run-scan-path")]
pub async fn run_scan_path_endpoint(
    query: web::Query<ScanPathQuery>,
    data: web::Data<AppState>,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    crate::local::scan(
        &query.path,
        data.database.clone(),
        crate::CANCELLATION_TOKEN.clone(),
    )
    .await
    .map_err(|e| ErrorInternalServerError(format!("Failed to scan: {e:?}")))?;

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetScanOriginsQuery {}

#[get("/scan-origins")]
pub async fn get_scan_origins_endpoint(
    _query: web::Query<GetScanOriginsQuery>,
    data: web::Data<AppState>,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    let origins = get_scan_origins(&**data.database)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to get scan origins: {e:?}")))?;

    Ok(Json(serde_json::json!({"origins": origins})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EnableScanOriginQuery {
    origin: ScanOrigin,
}

#[post("/scan-origins")]
pub async fn enable_scan_origin_endpoint(
    query: web::Query<EnableScanOriginQuery>,
    data: web::Data<AppState>,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    enable_scan_origin(&**data.database, query.origin)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to enable scan origin: {e:?}")))?;

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DisableScanOriginQuery {
    origin: ScanOrigin,
}

#[delete("/scan-origins")]
pub async fn disable_scan_origin_endpoint(
    query: web::Query<DisableScanOriginQuery>,
    data: web::Data<AppState>,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    disable_scan_origin(&**data.database, query.origin)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to disable scan origin: {e:?}")))?;

    Ok(Json(serde_json::json!({"success": true})))
}

#[cfg(feature = "local")]
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetScanPathsQuery {}

#[cfg(feature = "local")]
#[get("/scan-paths")]
pub async fn get_scan_paths_endpoint(
    _query: web::Query<GetScanPathsQuery>,
    data: web::Data<AppState>,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    let paths = crate::get_scan_paths(&**data.database)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to get scan paths: {e:?}")))?;

    Ok(Json(serde_json::json!({"paths": paths})))
}

#[cfg(feature = "local")]
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddScanPathQuery {
    path: String,
}

#[cfg(feature = "local")]
#[post("/scan-paths")]
pub async fn add_scan_path_endpoint(
    query: web::Query<AddScanPathQuery>,
    data: web::Data<AppState>,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    crate::add_scan_path(&**data.database, &query.path)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to add scan path: {e:?}")))?;

    Ok(Json(serde_json::json!({"success": true})))
}

#[cfg(feature = "local")]
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemoveScanPathQuery {
    path: String,
}

#[cfg(feature = "local")]
#[delete("/scan-paths")]
pub async fn remove_scan_path_endpoint(
    query: web::Query<RemoveScanPathQuery>,
    data: web::Data<AppState>,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    crate::remove_scan_path(&**data.database, &query.path)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to remove scan path: {e:?}")))?;

    Ok(Json(serde_json::json!({"success": true})))
}
