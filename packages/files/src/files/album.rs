use std::path::{Path, PathBuf};

use bytes::BytesMut;
use futures::{StreamExt, TryStreamExt};
use moosicbox_core::sqlite::{
    db::DbError,
    models::{Album, Id},
};
use moosicbox_database::{profiles::LibraryDatabase, query::*, DatabaseError};
use moosicbox_music_api::{AlbumError, ImageCoverSize, ImageCoverSource, MusicApi};
use moosicbox_stream_utils::stalled_monitor::StalledReadMonitor;
use thiserror::Error;
use tokio_util::codec::{BytesCodec, FramedRead};

use crate::{
    get_or_fetch_cover_bytes_from_remote_url, get_or_fetch_cover_from_remote_url,
    sanitize_filename, search_for_cover, CoverBytes, FetchCoverError,
};

fn get_album_cover_path(
    size: &str,
    source: &str,
    album_id: &str,
    artist_name: &str,
    album_name: &str,
) -> PathBuf {
    let path = moosicbox_config::get_cache_dir_path()
        .expect("Failed to get cache directory")
        .join(source)
        .join(sanitize_filename(artist_name))
        .join(sanitize_filename(album_name));

    let filename = format!("album_{album_id}_{size}.jpg");

    path.join(filename)
}

#[derive(Debug, Error)]
pub enum AlbumCoverError {
    #[error("Album cover not found for album: {0}")]
    NotFound(Id),
    #[error(transparent)]
    Album(#[from] AlbumError),
    #[error(transparent)]
    FetchCover(#[from] FetchCoverError),
    #[error(transparent)]
    FetchLocalAlbumCover(#[from] FetchLocalAlbumCoverError),
    #[error(transparent)]
    IO(#[from] tokio::io::Error),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error("Failed to read file with path: {0} ({1})")]
    File(String, String),
    #[error("Invalid source")]
    InvalidSource,
}

pub async fn get_local_album_cover(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    album: &Album,
    size: ImageCoverSize,
) -> Result<String, AlbumCoverError> {
    let source = api
        .album_cover_source(album, size)
        .await?
        .ok_or_else(|| AlbumCoverError::NotFound(album.id.to_owned()))?;

    if let Ok(cover) =
        fetch_local_album_cover(db, album, source.clone(), album.directory.as_ref()).await
    {
        return Ok(cover);
    }

    if let Ok(cover) = get_remote_album_cover(album, source, size).await {
        log::debug!("Found {} artist cover", api.source());
        return copy_streaming_cover_to_local(db, album, cover).await;
    }

    Err(AlbumCoverError::NotFound(album.id.to_owned()))
}

pub async fn get_local_album_cover_bytes(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    album: &Album,
    size: ImageCoverSize,
    try_to_get_stream_size: bool,
) -> Result<CoverBytes, AlbumCoverError> {
    let source = api
        .album_cover_source(album, size)
        .await?
        .ok_or_else(|| AlbumCoverError::NotFound(album.id.to_owned()))?;

    if let Ok(cover) = fetch_local_album_cover_bytes(db, album, album.directory.as_ref()).await {
        return Ok(cover);
    }

    if let Ok(cover) =
        get_remote_album_cover_bytes(album, source, size, try_to_get_stream_size).await
    {
        return Ok(cover);
    }

    Err(AlbumCoverError::NotFound(album.id.to_owned()))
}

#[derive(Debug, Error)]
pub enum FetchLocalAlbumCoverError {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error("No Album Cover")]
    NoAlbumCover,
    #[error("Invalid source")]
    InvalidSource,
}

async fn fetch_local_album_cover(
    db: &LibraryDatabase,
    album: &Album,
    source: ImageCoverSource,
    directory: Option<&String>,
) -> Result<String, FetchLocalAlbumCoverError> {
    match source {
        ImageCoverSource::LocalFilePath(cover) => {
            let cover_path = std::path::PathBuf::from(&cover);

            if Path::is_file(&cover_path) {
                return Ok(cover_path.to_str().unwrap().to_string());
            }

            let directory = directory.ok_or(FetchLocalAlbumCoverError::NoAlbumCover)?;
            let directory_path = std::path::PathBuf::from(directory);

            if let Some(path) = search_for_cover(directory_path, "cover", None, None).await? {
                let artwork = path.to_str().unwrap().to_string();

                log::debug!(
                    "Updating Album {} artwork file from '{cover}' to '{artwork}'",
                    &album.id
                );

                db.update("albums")
                    .where_eq("id", &album.id)
                    .value("artwork", artwork)
                    .execute(db)
                    .await?;

                return Ok(path.to_str().unwrap().to_string());
            }

            Err(FetchLocalAlbumCoverError::NoAlbumCover)
        }
        ImageCoverSource::RemoteUrl(_) => Err(FetchLocalAlbumCoverError::InvalidSource),
    }
}

async fn fetch_local_album_cover_bytes(
    db: &LibraryDatabase,
    album: &Album,
    directory: Option<&String>,
) -> Result<CoverBytes, FetchLocalAlbumCoverError> {
    let cover = album
        .artwork
        .as_ref()
        .ok_or(FetchLocalAlbumCoverError::NoAlbumCover)?;

    let cover_path = std::path::PathBuf::from(&cover);

    if Path::is_file(&cover_path) {
        let file = tokio::fs::File::open(cover_path.to_path_buf()).await?;

        let size = if let Ok(metadata) = file.metadata().await {
            Some(metadata.len())
        } else {
            None
        };

        return Ok(CoverBytes {
            stream: StalledReadMonitor::new(
                FramedRead::new(file, BytesCodec::new())
                    .map_ok(BytesMut::freeze)
                    .boxed(),
            ),
            size,
        });
    }

    let directory = directory.ok_or(FetchLocalAlbumCoverError::NoAlbumCover)?;
    let directory_path = std::path::PathBuf::from(directory);

    if let Some(path) = search_for_cover(directory_path, "cover", None, None).await? {
        let artwork = path.to_str().unwrap().to_string();

        log::debug!(
            "Updating Album {} artwork file from '{cover}' to '{artwork}'",
            &album.id
        );

        db.update("albums")
            .where_eq("id", &album.id)
            .value("artwork", artwork)
            .execute(db)
            .await?;

        let file = tokio::fs::File::open(path).await?;

        let size = if let Ok(metadata) = file.metadata().await {
            Some(metadata.len())
        } else {
            None
        };

        return Ok(CoverBytes {
            stream: StalledReadMonitor::new(
                FramedRead::new(file, BytesCodec::new())
                    .map_ok(BytesMut::freeze)
                    .boxed(),
            ),
            size,
        });
    }

    Err(FetchLocalAlbumCoverError::NoAlbumCover)
}

async fn copy_streaming_cover_to_local(
    db: &LibraryDatabase,
    album: &Album,
    cover: String,
) -> Result<String, AlbumCoverError> {
    log::debug!("Updating Album {} cover file to '{cover}'", album.id);

    db.update("albums")
        .where_eq("id", &album.id)
        .value("artwork", cover.clone())
        .execute(db)
        .await?;

    Ok(cover)
}

pub async fn get_album_cover(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    album: &Album,
    size: ImageCoverSize,
) -> Result<String, AlbumCoverError> {
    get_local_album_cover(api, db, album, size).await
}

pub async fn get_album_cover_bytes(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    album: &Album,
    size: ImageCoverSize,
    try_to_get_stream_size: bool,
) -> Result<CoverBytes, AlbumCoverError> {
    get_local_album_cover_bytes(api, db, album, size, try_to_get_stream_size).await
}

async fn get_remote_album_cover_request(
    album: &Album,
    source: ImageCoverSource,
    size: ImageCoverSize,
) -> Result<AlbumCoverRequest, AlbumCoverError> {
    match source {
        ImageCoverSource::LocalFilePath(_) => Err(AlbumCoverError::InvalidSource),
        ImageCoverSource::RemoteUrl(url) => {
            let file_path = get_album_cover_path(
                &size.to_string(),
                album.source.as_ref(),
                &album.id.to_string(),
                &album.artist,
                &album.title,
            );

            Ok(AlbumCoverRequest { url, file_path })
        }
    }
}

async fn get_remote_album_cover(
    album: &Album,
    source: ImageCoverSource,
    size: ImageCoverSize,
) -> Result<String, AlbumCoverError> {
    let request = get_remote_album_cover_request(album, source, size).await?;

    Ok(get_or_fetch_cover_from_remote_url(&request.url, &request.file_path).await?)
}

async fn get_remote_album_cover_bytes(
    album: &Album,
    source: ImageCoverSource,
    size: ImageCoverSize,
    try_to_get_stream_size: bool,
) -> Result<CoverBytes, AlbumCoverError> {
    let request = get_remote_album_cover_request(album, source, size).await?;

    Ok(get_or_fetch_cover_bytes_from_remote_url(
        &request.url,
        &request.file_path,
        try_to_get_stream_size,
    )
    .await?)
}

struct AlbumCoverRequest {
    url: String,
    file_path: PathBuf,
}
