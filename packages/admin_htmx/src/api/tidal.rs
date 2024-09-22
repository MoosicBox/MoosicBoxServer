use std::sync::LazyLock;

use actix_htmx::{Htmx, TriggerType};
use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    route, web, Scope,
};
use base64::{engine::general_purpose, Engine as _};
use maud::{html, Markup};
use moosicbox_database::profiles::LibraryDatabase;
use moosicbox_tidal::{db::TidalConfigError, TidalDeviceAuthorizationTokenError};
use serde::Deserialize;
use urlencoding::encode;

static C1: &str = "elU0WEhWVms=";
static C2: &str = "YzJ0RFBvNHQ=";
static SP1: &str = "VkpLaERGcUpQcXZzUFZOQlY2dWtYVA==";
static SP2: &str = "Sm13bHZidHRQN3dsTWxyYzcyc2U0PQ==";

pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope.service(
        web::scope("/tidal")
            .service(get_settings_endpoint)
            .service(device_authorization_endpoint)
            .service(device_authorization_token_endpoint),
    )
}

static CLIENT_ID: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{}{}",
        std::str::from_utf8(&general_purpose::STANDARD.decode(C1).unwrap()).unwrap(),
        std::str::from_utf8(&general_purpose::STANDARD.decode(C2).unwrap()).unwrap()
    )
});

static CLIENT_SECRET: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{}{}",
        std::str::from_utf8(&general_purpose::STANDARD.decode(SP1).unwrap()).unwrap(),
        std::str::from_utf8(&general_purpose::STANDARD.decode(SP2).unwrap()).unwrap()
    )
});

#[route("auth/device-authorization", method = "POST")]
pub async fn device_authorization_endpoint(
    htmx: Htmx,
    db: LibraryDatabase,
) -> Result<Markup, actix_web::Error> {
    let response = moosicbox_tidal::device_authorization(CLIENT_ID.clone(), false)
        .await
        .map_err(ErrorInternalServerError)?;
    let device_code = response
        .get("deviceCode")
        .ok_or_else(|| ErrorInternalServerError("Missing device code"))?
        .as_str()
        .ok_or_else(|| ErrorInternalServerError("Invalid deviceCode"))?;
    let url = response
        .get("url")
        .ok_or_else(|| ErrorInternalServerError("Missing url"))?
        .as_str()
        .ok_or_else(|| ErrorInternalServerError("Invalid url"))?;

    device_authorization_token(&db, htmx, device_code, url)
        .await
        .map_err(ErrorInternalServerError)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceAuthorizationTokenQuery {
    device_code: String,
    url: String,
}

#[route("auth/device-authorization/token", method = "POST")]
pub async fn device_authorization_token_endpoint(
    htmx: Htmx,
    query: web::Query<DeviceAuthorizationTokenQuery>,
    db: LibraryDatabase,
) -> Result<Markup, actix_web::Error> {
    device_authorization_token(&db, htmx, &query.device_code, &query.url)
        .await
        .map_err(ErrorInternalServerError)
}

async fn device_authorization_token(
    db: &LibraryDatabase,
    htmx: Htmx,
    device_code: &str,
    url: &str,
) -> Result<Markup, TidalDeviceAuthorizationTokenError> {
    let response = moosicbox_tidal::device_authorization_token(
        db,
        CLIENT_ID.clone(),
        CLIENT_SECRET.clone(),
        device_code.to_owned(),
        Some(true),
    )
    .await;

    if response.is_ok_and(|x| x.get("accessToken").is_some()) {
        htmx.trigger_event(
            "tidal-login-attempt".to_string(),
            Some(
                r#"{"level": "info", "message": "Successfully logged in to Tidal", "success": true}"#
                    .to_string(),
            ),
            Some(TriggerType::Standard),
        );

        Ok(settings_logged_in())
    } else {
        htmx.trigger_event(
            "tidal-login-attempt".to_string(),
            Some(
                r#"{"level": "info", "message": "Failed to login to Tidal", "success": false}"#
                    .to_string(),
            ),
            Some(TriggerType::Standard),
        );

        Ok(html! {
            div
                hx-post={"/admin/tidal/auth/device-authorization/token?deviceCode="(encode(device_code))"&url="(encode(url))}
                hx-swap="outerHTML"
                hx-trigger="every 1s" {
                p {
                    "Follow this link to authenticate with Tidal: " a href=(url) target="_blank" { (url) }
                }
            }
        })
    }
}

#[route("settings", method = "GET", method = "OPTIONS", method = "HEAD")]
pub async fn get_settings_endpoint(
    _htmx: Htmx,
    db: LibraryDatabase,
) -> Result<Markup, actix_web::Error> {
    settings(&db)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to get Tidal settings: {e:?}")))
}

pub fn settings_logged_in() -> Markup {
    html! {
        p { "Logged in!" }
    }
}

pub fn settings_logged_out() -> Markup {
    html! {
        form hx-post="/admin/tidal/auth/device-authorization" hx-swap="outerHTML" {
            button type="submit" { "Start web authentication" }
        }
    }
}

pub async fn settings(db: &LibraryDatabase) -> Result<Markup, TidalConfigError> {
    let logged_in = moosicbox_tidal::db::get_tidal_config(db).await?.is_some();

    Ok(if logged_in {
        settings_logged_in()
    } else {
        settings_logged_out()
    })
}