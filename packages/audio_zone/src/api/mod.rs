use actix_web::{
    error::ErrorInternalServerError,
    route,
    web::{self, Json},
    Result,
};
use moosicbox_database::TryIntoDb as _;
use moosicbox_paging::Page;
use serde::Deserialize;

use crate::models::{ApiAudioZone, AudioZone, CreateAudioZone, UpdateAudioZone};

pub mod models;

#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "Audio Zone")),
    paths(
        audio_zones_endpoint,
        create_audio_zone_endpoint,
        update_audio_zone_endpoint,
    ),
    components(schemas(
        ApiAudioZone,
        UpdateAudioZone,
        crate::models::ApiPlayer,
    ))
)]
pub struct Api;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAudioZones {
    offset: Option<u32>,
    limit: Option<u32>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Audio Zone"],
        get,
        path = "",
        description = "Get a list of the enabled audio zones",
        params(
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
        ),
        responses(
            (
                status = 200,
                description = "A paginated response of audio zones",
                body = Value,
            )
        )
    )
)]
#[route("", method = "GET")]
pub async fn audio_zones_endpoint(
    query: web::Query<GetAudioZones>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiAudioZone>>> {
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(30);
    let zones = crate::zones(&**data.database)
        .await
        .map_err(ErrorInternalServerError)?;
    let total = zones.len() as u32;
    let zones = zones
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(|x| x.into())
        .collect::<Vec<_>>();

    Ok(Json(Page::WithTotal {
        items: zones,
        offset,
        limit,
        total,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAudioZoneQuery {
    pub name: String,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Audio Zone"],
        post,
        path = "",
        description = "Create a new audio zone",
        params(
            ("name" = String, Query, description = "Name of the audio zone to create"),
        ),
        responses(
            (
                status = 200,
                description = "The audio zone that was successfully created",
                body = ApiAudioZone,
            )
        )
    )
)]
#[route("", method = "POST")]
pub async fn create_audio_zone_endpoint(
    query: web::Query<CreateAudioZoneQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<ApiAudioZone>> {
    let create = CreateAudioZone {
        name: query.name.clone(),
    };
    let zone = crate::create_audio_zone(&**data.database, &create)
        .await
        .map_err(ErrorInternalServerError)?;
    let zone: AudioZone = zone
        .try_into_db(&**data.database)
        .await
        .map_err(ErrorInternalServerError)?;
    let zone = zone.into();

    Ok(Json(zone))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAudioZoneQuery {}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Audio Zone"],
        patch,
        path = "",
        request_body = UpdateAudioZone,
        description = "Update an existing audio zone",
        params(),
        responses(
            (
                status = 200,
                description = "The audio zone that was successfully updated",
                body = ApiAudioZone,
            )
        )
    )
)]
#[route("", method = "PATCH")]
pub async fn update_audio_zone_endpoint(
    update: Json<UpdateAudioZone>,
    _query: web::Query<UpdateAudioZoneQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<ApiAudioZone>> {
    let zone = crate::update_audio_zone(&**data.database, update.clone())
        .await
        .map_err(ErrorInternalServerError)?;
    let zone: AudioZone = zone
        .try_into_db(&**data.database)
        .await
        .map_err(ErrorInternalServerError)?;
    let zone = zone.into();

    Ok(Json(zone))
}
