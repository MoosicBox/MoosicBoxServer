use moosicbox_core::sqlite::db::{select, upsert, DbError, SqliteValue};
use rusqlite::Connection;

pub mod models;

use self::models::{CreateDownloadTask, DownloadItem, DownloadLocation, DownloadTask};

pub fn create_download_location(db: &Connection, path: &str) -> Result<(), DbError> {
    upsert::<DownloadLocation>(
        db,
        "download_locations",
        vec![("path", SqliteValue::String(path.to_string()))],
        vec![("path", SqliteValue::String(path.to_string()))],
    )?;

    Ok(())
}

pub fn get_download_location(
    db: &Connection,
    id: u64,
) -> Result<Option<DownloadLocation>, DbError> {
    Ok(select::<DownloadLocation>(
        db,
        "download_locations",
        &vec![("id", SqliteValue::Number(id as i64))],
        &["*"],
    )?
    .into_iter()
    .next())
}

pub fn get_download_locations(db: &Connection) -> Result<Vec<DownloadLocation>, DbError> {
    let download_locations = select::<DownloadLocation>(db, "download_locations", &vec![], &["*"])?
        .into_iter()
        .collect::<Vec<_>>();

    Ok(download_locations)
}

pub fn create_download_task(
    db: &Connection,
    task: &CreateDownloadTask,
) -> Result<DownloadTask, DbError> {
    upsert::<DownloadTask>(
        db,
        "download_tasks",
        vec![("file_path", SqliteValue::String(task.file_path.clone()))],
        vec![
            ("file_path", SqliteValue::String(task.file_path.clone())),
            (
                "source",
                SqliteValue::StringOpt(
                    task.source
                        .as_ref()
                        .map(|source| source.as_ref().to_string()),
                ),
            ),
            ("type", SqliteValue::String(task.item.as_ref().to_string())),
            (
                "track_id",
                SqliteValue::NumberOpt(if let DownloadItem::Track(track_id) = task.item {
                    Some(track_id as i64)
                } else {
                    None
                }),
            ),
            (
                "album_id",
                SqliteValue::NumberOpt(if let DownloadItem::AlbumCover(album_id) = task.item {
                    Some(album_id as i64)
                } else if let DownloadItem::ArtistCover(album_id) = task.item {
                    Some(album_id as i64)
                } else {
                    None
                }),
            ),
        ],
    )
}

pub fn get_download_tasks(db: &Connection) -> Result<Vec<DownloadTask>, DbError> {
    let download_tasks = select::<DownloadTask>(db, "download_tasks", &vec![], &["*"])?
        .into_iter()
        .collect::<Vec<_>>();

    Ok(download_tasks)
}
