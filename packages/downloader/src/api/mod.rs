use std::sync::Arc;

use crate::api::models::ApiDownloadTask;
use crate::api::models::ApiDownloadTaskState;
use crate::create_download_tasks;
use crate::db::get_download_tasks;
use crate::db::models::DownloadTaskState;
use crate::get_create_download_tasks;
use crate::get_download_path;
use crate::queue::{DownloadQueue, ProcessDownloadQueueError, ProgressListenerRef};
use crate::CreateDownloadTasksError;
use crate::DownloadApiSource;
use crate::GetCreateDownloadTasksError;
use crate::GetDownloadPathError;
use crate::MoosicboxDownloader;
use actix_web::error::ErrorBadRequest;
use actix_web::error::ErrorInternalServerError;
use actix_web::error::ErrorNotFound;
use actix_web::{
    route,
    web::{self, Json},
    Result,
};
use moosicbox_core::sqlite::models::Id;
use moosicbox_database::Database;
use moosicbox_music_api::MusicApiState;
use moosicbox_music_api::TrackAudioQuality;
use moosicbox_paging::Page;
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::RwLock;

pub mod models;

#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "Downloader")),
    paths(download_endpoint, retry_download_endpoint, download_tasks_endpoint),
    components(schemas(DownloadApiSource, Id, TrackAudioQuality))
)]
pub struct Api;

static DOWNLOAD_QUEUE: Lazy<Arc<RwLock<DownloadQueue>>> =
    Lazy::new(|| Arc::new(RwLock::new(DownloadQueue::new())));

pub async fn add_progress_listener_to_download_queue(listener: ProgressListenerRef) {
    DOWNLOAD_QUEUE.write().await.add_progress_listener(listener);
}

async fn get_default_download_queue(
    db: Arc<Box<dyn Database>>,
    api_state: MusicApiState,
) -> Arc<RwLock<DownloadQueue>> {
    let queue = { DOWNLOAD_QUEUE.read().await.clone() };

    if !queue.has_database() {
        let mut queue = DOWNLOAD_QUEUE.write().await;
        let output = queue.with_database(db.clone());
        *queue = output.clone();
    }
    if !queue.has_downloader() {
        let mut queue = DOWNLOAD_QUEUE.write().await;
        let output = queue.with_downloader(Box::new(MoosicboxDownloader::new(db, api_state)));
        *queue = output.clone();
    }

    DOWNLOAD_QUEUE.clone()
}

impl From<GetDownloadPathError> for actix_web::Error {
    fn from(err: GetDownloadPathError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

impl From<GetCreateDownloadTasksError> for actix_web::Error {
    fn from(err: GetCreateDownloadTasksError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

impl From<CreateDownloadTasksError> for actix_web::Error {
    fn from(err: CreateDownloadTasksError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

impl From<ProcessDownloadQueueError> for actix_web::Error {
    fn from(err: ProcessDownloadQueueError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadQuery {
    location_id: Option<u64>,
    track_id: Option<Id>,
    track_ids: Option<String>,
    album_id: Option<Id>,
    album_ids: Option<String>,
    download_album_cover: Option<bool>,
    download_artist_cover: Option<bool>,
    quality: Option<TrackAudioQuality>,
    source: DownloadApiSource,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Downloader"],
        post,
        path = "/download",
        description = "Queue the specified tracks or albums to be downloaded",
        params(
            ("locationId" = Option<u64>, Query, description = "The download location to save the files to"),
            ("trackId" = Option<Id>, Query, description = "A trackId to download"),
            ("trackIds" = Option<String>, Query, description = "A comma-separated list of trackIds to download"),
            ("albumId" = Option<Id>, Query, description = "A albumId to download"),
            ("albumIds" = Option<String>, Query, description = "A comma-separated list of albumIds to download"),
            ("downloadAlbumCover" = Option<bool>, Query, description = "Whether or not to download the album cover, if available"),
            ("downloadArtistCover" = Option<bool>, Query, description = "Whether or not to download the artist cover, if available"),
            ("quality" = Option<TrackAudioQuality>, Query, description = "The track audio quality to download the tracks at"),
            ("source" = DownloadApiSource, Query, description = "The API source to download the track from"),
        ),
        responses(
            (
                status = 200,
                description = "The download has successfully started",
                body = Value,
            )
        )
    )
)]
#[route("/download", method = "POST")]
pub async fn download_endpoint(
    query: web::Query<DownloadQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
    api_state: web::Data<MusicApiState>,
) -> Result<Json<Value>> {
    let download_path = get_download_path(&**data.database, query.location_id).await?;

    let tasks = get_create_download_tasks(
        &**api_state
            .apis
            .get(query.source.into())
            .map_err(|e| ErrorBadRequest(format!("Invalid source: {e:?}")))?,
        &download_path,
        query.track_id.clone(),
        query.track_ids.clone(),
        query.album_id.clone(),
        query.album_ids.clone(),
        query.download_album_cover.unwrap_or(true),
        query.download_artist_cover.unwrap_or(true),
        query.quality,
        Some(query.source),
    )
    .await?;

    let tasks = create_download_tasks(&**data.database, tasks).await?;

    let queue = get_default_download_queue(data.database.clone(), api_state.as_ref().clone()).await;
    let mut download_queue = queue.write().await;

    download_queue.add_tasks_to_queue(tasks).await;
    download_queue.process();

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryDownloadQuery {
    task_id: u64,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Downloader"],
        post,
        path = "/retry-download",
        description = "Retry a specific download task",
        params(
            ("taskId" = u64, Query, description = "The task ID to retry"),
        ),
        responses(
            (
                status = 200,
                description = "Retry a specific download task",
                body = Value,
            )
        )
    )
)]
#[route("/retry-download", method = "POST")]
pub async fn retry_download_endpoint(
    query: web::Query<RetryDownloadQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
    api_state: web::Data<MusicApiState>,
) -> Result<Json<Value>> {
    let tasks = get_download_tasks(&**data.database).await?;
    let task = tasks
        .into_iter()
        .find(|x| x.id == query.task_id)
        .ok_or_else(|| ErrorNotFound(format!("Task not found with ID {}", query.task_id)))?;

    let mut download_queue = DownloadQueue::new();
    download_queue.with_database(data.database.clone());
    download_queue.with_downloader(Box::new(MoosicboxDownloader::new(
        data.database.clone(),
        api_state.as_ref().clone(),
    )));
    download_queue.add_tasks_to_queue(vec![task]).await;
    download_queue.process();

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDownloadTasks {
    offset: Option<u32>,
    limit: Option<u32>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Downloader"],
        get,
        path = "/download-tasks",
        description = "Get a list of the current and historical download tasks",
        params(
            ("offset" = Option<u32>, Query, description = "Page offset"),
            ("limit" = Option<u32>, Query, description = "Page limit"),
        ),
        responses(
            (
                status = 200,
                description = "A paginated response of download items",
                body = Value,
            )
        )
    )
)]
#[route("/download-tasks", method = "GET")]
pub async fn download_tasks_endpoint(
    query: web::Query<GetDownloadTasks>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Page<ApiDownloadTask>>> {
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(30);
    let tasks = get_download_tasks(&**data.database).await?;
    let (mut current, mut history): (Vec<_>, Vec<_>) =
        tasks.into_iter().partition(|task| match task.state {
            DownloadTaskState::Pending | DownloadTaskState::Paused | DownloadTaskState::Started => {
                true
            }
            DownloadTaskState::Cancelled
            | DownloadTaskState::Finished
            | DownloadTaskState::Error => false,
        });

    current.sort_by(|a, b| a.id.cmp(&b.id));
    history.sort_by(|a, b| b.id.cmp(&a.id));

    let tasks = [current, history].concat();
    let total = tasks.len() as u32;
    let mut tasks = tasks
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(|x| x.into())
        .collect::<Vec<ApiDownloadTask>>();

    for task in tasks.iter_mut() {
        if task.state == ApiDownloadTaskState::Started {
            task.speed
                .replace(DOWNLOAD_QUEUE.read().await.speed().unwrap_or(0.0) as u64);
        }
    }

    Ok(Json(Page::WithTotal {
        offset,
        items: tasks,
        limit,
        total,
    }))
}
