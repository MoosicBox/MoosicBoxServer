use actix_web::{
    error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound},
    route,
    web::{self, Json},
    HttpRequest, HttpResponse, Result,
};
use futures::StreamExt;
use moosicbox_core::{
    app::AppState,
    integer_range::{parse_integer_ranges_to_ids, ParseIntegersError},
    sqlite::models::{ApiSource, Id},
    types::AudioFormat,
};
use moosicbox_music_api::{ImageCoverSize, MusicApiState, TrackAudioQuality, TrackSource};
use serde::Deserialize;

use crate::files::{
    album::{get_album_cover, AlbumCoverError},
    artist::{get_artist_cover, ArtistCoverError},
    resize_image_path,
    track::{
        audio_format_to_content_type, get_or_init_track_visualization, get_track_bytes,
        get_track_id_source, get_track_info, get_tracks_info, track_source_to_content_type,
        GetTrackBytesError, TrackInfo, TrackInfoError, TrackSourceError,
    },
};

#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "Files")),
    paths(
        track_visualization_endpoint,
        track_endpoint,
        track_info_endpoint,
        tracks_info_endpoint,
        artist_cover_endpoint,
        artist_source_artwork_endpoint,
        album_artwork_endpoint,
        album_source_artwork_endpoint,
    ),
    components(schemas(
        GetTrackVisualizationQuery,
        GetTrackQuery,
        GetTrackInfoQuery,
        GetTracksInfoQuery,
        ArtistCoverQuery,
        AlbumCoverQuery,
        TrackInfo,
        AudioFormat,
        ApiSource,
    ))
)]
pub struct Api;

impl From<TrackSourceError> for actix_web::Error {
    fn from(e: TrackSourceError) -> Self {
        match e {
            TrackSourceError::NotFound(e) => ErrorNotFound(e.to_string()),
            TrackSourceError::InvalidSource => ErrorBadRequest(e.to_string()),
            TrackSourceError::Track(_)
            | TrackSourceError::Db(_)
            | TrackSourceError::MusicApis(_) => ErrorInternalServerError(e.to_string()),
        }
    }
}

impl From<TrackInfoError> for actix_web::Error {
    fn from(e: TrackInfoError) -> Self {
        ErrorInternalServerError(e.to_string())
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GetTrackVisualizationQuery {
    pub track_id: u64,
    pub max: Option<u16>,
    pub source: Option<ApiSource>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Files"],
        get,
        path = "/track/visualization",
        description = "Get the track visualization data points",
        params(
            ("trackId" = u64, Query,
                description = "The track ID"),
            ("max" = Option<u16>, Query,
                description = "The maximum number of visualization data points to return"),
            ("source" = Option<ApiSource>, Query,
                description = "The track API source"),
        ),
        responses(
            (
                status = 200,
                description = "Track audio bytes",
                body = Vec<u8>,
            )
        )
    )
)]
#[route("/track/visualization", method = "GET")]
pub async fn track_visualization_endpoint(
    query: web::Query<GetTrackVisualizationQuery>,
    api_state: web::Data<MusicApiState>,
) -> Result<Json<Vec<u8>>> {
    let source = get_track_id_source(
        api_state.apis.clone(),
        &query.track_id.into(),
        query.source.unwrap_or(ApiSource::Library),
        Some(TrackAudioQuality::Low),
    )
    .await?;

    Ok(Json(
        get_or_init_track_visualization(&source, query.max.unwrap_or(333)).await?,
    ))
}

impl From<GetTrackBytesError> for actix_web::Error {
    fn from(err: GetTrackBytesError) -> Self {
        match err {
            GetTrackBytesError::Db(_)
            | GetTrackBytesError::IO(_)
            | GetTrackBytesError::Reqwest(_)
            | GetTrackBytesError::Acquire(_)
            | GetTrackBytesError::Join(_)
            | GetTrackBytesError::ToStr(_)
            | GetTrackBytesError::ParseInt(_)
            | GetTrackBytesError::Recv(_)
            | GetTrackBytesError::Commander(_)
            | GetTrackBytesError::Track(_)
            | GetTrackBytesError::TrackInfo(_) => ErrorInternalServerError(err),
            GetTrackBytesError::NotFound => ErrorNotFound(err),
            GetTrackBytesError::UnsupportedFormat => ErrorBadRequest(err),
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GetTrackQuery {
    pub track_id: u64,
    pub format: Option<AudioFormat>,
    pub quality: Option<TrackAudioQuality>,
    pub source: Option<ApiSource>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Files", "head"],
        get,
        path = "/track",
        description = "Get the track file stream audio bytes with a chunked encoding",
        params(
            ("trackId" = u64, Query, description = "The track ID"),
            ("format" = Option<AudioFormat>, Query, description = "The track format to return"),
            ("quality" = Option<TrackAudioQuality>, Query, description = "The quality to return"),
            ("source" = Option<ApiSource>, Query, description = "The track API source"),
        ),
        responses(
            (
                status = 200,
                description = "Track audio bytes",
            )
        )
    )
)]
#[route("/track", method = "GET", method = "HEAD")]
pub async fn track_endpoint(
    req: HttpRequest,
    query: web::Query<GetTrackQuery>,
    api_state: web::Data<MusicApiState>,
) -> Result<HttpResponse> {
    let method = req.method();

    let source = get_track_id_source(
        api_state.apis.clone(),
        &query.track_id.into(),
        query.source.unwrap_or(ApiSource::Library),
        query.quality,
    )
    .await?;

    log::debug!(
        "{method} /track track_id={} quality={:?} query.source={:?} source={source:?}",
        query.track_id,
        query.quality,
        query.source
    );

    let content_type = query
        .format
        .as_ref()
        .and_then(audio_format_to_content_type)
        .or(track_source_to_content_type(&source));

    let format = query.format.unwrap_or_default();

    #[cfg(feature = "track-range")]
    let range = req
        .headers()
        .get(actix_web::http::header::RANGE)
        .and_then(|x| x.to_str().ok())
        .map(|range| {
            log::debug!("Got range request {:?}", range);

            range
                .strip_prefix("bytes=")
                .map(|s| s.to_string())
                .ok_or(ErrorBadRequest(format!("Invalid range: {range}")))
        })
        .transpose()?
        .map(|range| {
            crate::range::parse_range(&range)
                .map_err(|e| ErrorBadRequest(format!("Invalid bytes range: {range} ({e:?})")))
        })
        .transpose()?;

    #[cfg(not(feature = "track-range"))]
    let range: Option<crate::range::Range> = None;

    let mut response = HttpResponse::Ok();
    response.insert_header((actix_web::http::header::ACCEPT_RANGES, "bytes"));
    response.insert_header((actix_web::http::header::CONTENT_ENCODING, "identity"));

    if let Some(content_type) = content_type {
        response.insert_header((actix_web::http::header::CONTENT_TYPE, content_type));
    } else {
        match &source {
            TrackSource::RemoteUrl { .. } => {
                #[cfg(feature = "flac")]
                {
                    response.insert_header((
                        actix_web::http::header::CONTENT_TYPE,
                        audio_format_to_content_type(&AudioFormat::Flac).unwrap(),
                    ));
                }
                #[cfg(not(feature = "flac"))]
                {
                    moosicbox_assert::die_or_warn!(
                        "No valid CONTENT_TYPE available for audio format {format:?}"
                    );
                }
            }
            _ => {
                moosicbox_assert::die_or_warn!("Failed to get CONTENT_TYPE for track source");
            }
        }
    }

    log::debug!("{method} /track Fetching track bytes with range range={range:?}");

    let bytes = get_track_bytes(
        &**api_state
            .apis
            .get(query.source.unwrap_or(ApiSource::Library))
            .map_err(|e| ErrorBadRequest(format!("Invalid source: {e:?}")))?,
        &Id::Number(query.track_id),
        source,
        format,
        true,
        range.as_ref().and_then(|r| r.start.map(|x| x as u64)),
        range.as_ref().and_then(|r| r.end.map(|x| x as u64)),
    )
    .await?;

    response.insert_header((
        actix_web::http::header::CONTENT_DISPOSITION,
        format!(
            "inline{filename}",
            filename = bytes
                .filename
                .map_or("".into(), |x| format!("; filename=\"{x}\""))
        ),
    ));

    log::debug!(
        "Got bytes with size={:?} original_size={:?}",
        bytes.size,
        bytes.original_size
    );

    let stream = bytes.stream.filter_map(|x| async { x.ok() });

    if let Some(original_size) = bytes.original_size {
        let size = if let Some(range) = range {
            let mut size = original_size;
            if let Some(end) = range.end {
                if end > size as usize {
                    let error = format!("Range end out of bounds: {end}");
                    log::error!("{}", error);
                    return Err(ErrorBadRequest(error));
                }
                size = end as u64;
            }
            if let Some(start) = range.start {
                if start > size as usize {
                    let error = format!("Range start out of bounds: {start}");
                    log::error!("{}", error);
                    return Err(ErrorBadRequest(error));
                }
                size -= start as u64;
            }

            response.insert_header((
                actix_web::http::header::CONTENT_RANGE,
                format!(
                    "bytes {start}-{end}/{original_size}",
                    start = range.start.map_or("".to_string(), |x| x.to_string()),
                    end = range.end.map(|x| x as u64).unwrap_or(original_size - 1),
                ),
            ));
            size
        } else {
            response.insert_header((
                actix_web::http::header::CONTENT_RANGE,
                format!("bytes -{end}/{original_size}", end = original_size - 1),
            ));
            original_size
        };

        if size != original_size {
            response.status(actix_web::http::StatusCode::PARTIAL_CONTENT);
        }

        log::debug!("Returning stream body with size={:?}", size);
        Ok(response.body(actix_web::body::SizedStream::new(size, stream)))
    } else {
        log::debug!("No size was found for stream");
        Ok(response.streaming(stream))
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GetTrackInfoQuery {
    pub track_id: Id,
    pub source: Option<ApiSource>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Files"],
        get,
        path = "/track/info",
        description = "Get the track's metadata",
        params(
            ("trackId" = Id, Query, description = "The track ID"),
            ("source" = Option<ApiSource>, Query, description = "The track API source"),
        ),
        responses(
            (
                status = 200,
                description = "Track info",
                body = TrackInfo,
            )
        )
    )
)]
#[route("/track/info", method = "GET")]
pub async fn track_info_endpoint(
    query: web::Query<GetTrackInfoQuery>,
    api_state: web::Data<MusicApiState>,
) -> Result<Json<TrackInfo>> {
    Ok(Json(
        get_track_info(
            &**api_state
                .apis
                .get(query.source.unwrap_or(ApiSource::Library))
                .map_err(|e| ErrorBadRequest(format!("Invalid source: {e:?}")))?,
            &query.track_id,
        )
        .await?,
    ))
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GetTracksInfoQuery {
    pub track_ids: String,
    pub source: Option<ApiSource>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Files"],
        get,
        path = "/tracks/info",
        description = "Get the track's info",
        params(
            ("trackIds" = String, Query, description = "The comma-separated list of track IDs"),
            ("source" = Option<ApiSource>, Query, description = "The tracks' API source"),
        ),
        responses(
            (
                status = 200,
                description = "Tracks info",
                body = Vec<TrackInfo>,
            )
        )
    )
)]
#[route("/tracks/info", method = "GET")]
pub async fn tracks_info_endpoint(
    query: web::Query<GetTracksInfoQuery>,
    api_state: web::Data<MusicApiState>,
) -> Result<Json<Vec<TrackInfo>>> {
    let ids = parse_integer_ranges_to_ids(&query.track_ids).map_err(|e| match e {
        ParseIntegersError::ParseId(id) => {
            ErrorBadRequest(format!("Could not parse trackId '{id}'"))
        }
        ParseIntegersError::UnmatchedRange(range) => {
            ErrorBadRequest(format!("Unmatched range '{range}'"))
        }
        ParseIntegersError::RangeTooLarge(range) => {
            ErrorBadRequest(format!("Range too large '{range}'"))
        }
    })?;

    Ok(Json(
        get_tracks_info(
            &**api_state
                .apis
                .get(query.source.unwrap_or(ApiSource::Library))
                .map_err(|e| ErrorBadRequest(format!("Invalid source: {e:?}")))?,
            &ids,
        )
        .await?,
    ))
}

impl From<ArtistCoverError> for actix_web::Error {
    fn from(err: ArtistCoverError) -> Self {
        match err {
            ArtistCoverError::NotFound(..) => ErrorNotFound(err.to_string()),
            ArtistCoverError::Artist(_)
            | ArtistCoverError::FetchCover(_)
            | ArtistCoverError::FetchLocalArtistCover(_)
            | ArtistCoverError::IO(_)
            | ArtistCoverError::Db(_)
            | ArtistCoverError::Database(_)
            | ArtistCoverError::File(_, _)
            | ArtistCoverError::InvalidSource => ErrorInternalServerError(err.to_string()),
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ArtistCoverQuery {
    pub source: Option<ApiSource>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Files", "head"],
        get,
        path = "/artists/{artistId}/source",
        description = "Get source quality artist cover",
        params(
            ("artistId" = u64, Path, description = "The Artist ID"),
        ),
        responses(
            (
                status = 200,
                description = "The source quality artist cover",
            )
        )
    )
)]
#[route("/artists/{artist_id}/source", method = "GET", method = "HEAD")]
pub async fn artist_source_artwork_endpoint(
    req: HttpRequest,
    path: web::Path<String>,
    query: web::Query<ArtistCoverQuery>,
    data: web::Data<AppState>,
    api_state: web::Data<MusicApiState>,
) -> Result<HttpResponse> {
    let paths = path.into_inner();

    let artist_id_string = paths;
    let source = query.source.unwrap_or(ApiSource::Library);
    let artist_id = match source {
        ApiSource::Library => artist_id_string.parse::<u64>().map(Id::Number),
        ApiSource::Tidal => artist_id_string.parse::<u64>().map(Id::Number),
        ApiSource::Qobuz => Ok(Id::String(artist_id_string)),
        ApiSource::Yt => Ok(Id::String(artist_id_string)),
    }
    .map_err(|_e| ErrorBadRequest("Invalid artist_id"))?;

    let size = ImageCoverSize::Max;
    let api = api_state
        .apis
        .get(source)
        .map_err(|e| ErrorInternalServerError(format!("Failed to get music_api: {e:?}")))?;
    let artist = api
        .artist(&artist_id)
        .await
        .map_err(|e| ErrorNotFound(format!("Failed to get artist: {e:?}")))?
        .ok_or_else(|| ErrorNotFound(format!("Artist not found: {}", artist_id.to_owned())))?;

    log::debug!("artist_source_cover_endpoint: artist={artist:?}");

    let path = get_artist_cover(&**api, &**data.database, &artist, size).await?;
    let path_buf = std::path::PathBuf::from(path);
    let file_path = path_buf.as_path();

    let file = actix_files::NamedFile::open_async(file_path)
        .await
        .map_err(|e| ArtistCoverError::File(file_path.to_str().unwrap().into(), format!("{e:?}")))
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;

    Ok(file.into_response(&req))
}

#[cfg(feature = "openapi")]
#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Files", "head"],
        get,
        path = "/artists/{artistId}/{size}",
        description = "Get artist cover at the specified dimensions",
        params(
            ("artistId" = u64, Path, description = "The Artist ID"),
            ("size" = String, Path, description = "The {width}x{height} of the image"),
        ),
        responses(
            (
                status = 200,
                description = "The artist cover at the specified dimensions",
            )
        )
    )
)]
#[route("/artists/{artist_id}/{size}", method = "GET", method = "HEAD")]
#[allow(unused)]
pub async fn artist_cover_head_endpoint(
    _path: web::Path<(String, String)>,
    _query: web::Query<ArtistCoverQuery>,
    _data: web::Data<AppState>,
    _api_state: web::Data<MusicApiState>,
) -> Result<HttpResponse> {
    unimplemented!()
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Files", "head"],
        get,
        path = "/artists/{artistId}/{size}",
        description = "Get artist cover at the specified dimensions",
        params(
            ("artistId" = u64, Path, description = "The Artist ID"),
            ("size" = String, Path, description = "The {width}x{height} of the image"),
        ),
        responses(
            (
                status = 200,
                description = "The artist cover at the specified dimensions",
            )
        )
    )
)]
#[route("/artists/{artist_id}/{size}", method = "GET", method = "HEAD")]
pub async fn artist_cover_endpoint(
    path: web::Path<(String, String)>,
    query: web::Query<ArtistCoverQuery>,
    data: web::Data<AppState>,
    api_state: web::Data<MusicApiState>,
) -> Result<HttpResponse> {
    let paths = path.into_inner();

    let artist_id_string = paths.0;
    let source = query.source.unwrap_or(ApiSource::Library);
    let artist_id = match source {
        ApiSource::Library => artist_id_string.parse::<u64>().map(Id::Number),
        ApiSource::Tidal => artist_id_string.parse::<u64>().map(Id::Number),
        ApiSource::Qobuz => Ok(Id::String(artist_id_string)),
        ApiSource::Yt => Ok(Id::String(artist_id_string)),
    }
    .map_err(|_e| ErrorBadRequest("Invalid artist_id"))?;

    let dimensions = paths
        .1
        .split('x')
        .take(2)
        .map(|dimension| {
            dimension
                .parse::<u32>()
                .map_err(|_e| ErrorBadRequest("Invalid dimension"))
        })
        .collect::<Result<Vec<_>>>()?;
    let (width, height) = (dimensions[0], dimensions[1]);

    let size = (std::cmp::max(width, height) as u16).into();
    let api = api_state
        .apis
        .get(source)
        .map_err(|e| ErrorInternalServerError(format!("Failed to get music_api: {e:?}")))?;
    let artist = api
        .artist(&artist_id)
        .await
        .map_err(|e| ErrorNotFound(format!("Failed to get artist: {e:?}")))?
        .ok_or_else(|| ErrorNotFound(format!("Artist not found: {}", artist_id.to_owned())))?;

    log::debug!("artist_cover_endpoint: artist={artist:?}");

    let path = get_artist_cover(&**api, &**data.database, &artist, size).await?;

    resize_image_path(&path, width, height)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to resize image: {e:?}")))
}

impl From<AlbumCoverError> for actix_web::Error {
    fn from(err: AlbumCoverError) -> Self {
        match err {
            AlbumCoverError::NotFound(..) => ErrorNotFound(err.to_string()),
            AlbumCoverError::Album(_)
            | AlbumCoverError::FetchCover(_)
            | AlbumCoverError::FetchLocalAlbumCover(_)
            | AlbumCoverError::IO(_)
            | AlbumCoverError::Db(_)
            | AlbumCoverError::Database(_)
            | AlbumCoverError::File(_, _)
            | AlbumCoverError::InvalidSource => ErrorInternalServerError(err.to_string()),
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AlbumCoverQuery {
    pub source: Option<ApiSource>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Files", "head"],
        get,
        path = "/albums/{artistId}/source",
        description = "Get source quality album cover",
        params(
            ("artistId" = u64, Path, description = "The Artist ID"),
        ),
        responses(
            (
                status = 200,
                description = "The source quality album cover",
            )
        )
    )
)]
#[route("/albums/{album_id}/source", method = "GET", method = "HEAD")]
pub async fn album_source_artwork_endpoint(
    req: HttpRequest,
    path: web::Path<String>,
    query: web::Query<AlbumCoverQuery>,
    data: web::Data<AppState>,
    api_state: web::Data<MusicApiState>,
) -> Result<HttpResponse> {
    let paths = path.into_inner();

    let album_id_string = paths;
    let source = query.source.unwrap_or(ApiSource::Library);
    let album_id = match source {
        ApiSource::Library => album_id_string.parse::<u64>().map(Id::Number),
        ApiSource::Tidal => album_id_string.parse::<u64>().map(Id::Number),
        ApiSource::Qobuz => Ok(Id::String(album_id_string)),
        ApiSource::Yt => Ok(Id::String(album_id_string)),
    }
    .map_err(|_e| ErrorBadRequest("Invalid album_id"))?;

    let size = ImageCoverSize::Max;
    let api = api_state
        .apis
        .get(source)
        .map_err(|e| ErrorInternalServerError(format!("Failed to get music_api: {e:?}")))?;
    let album = api
        .album(&album_id)
        .await
        .map_err(|e| ErrorNotFound(format!("Failed to get album: {e:?}")))?
        .ok_or_else(|| ErrorNotFound(format!("Album not found: {}", album_id.to_owned())))?;

    log::debug!("album_source_cover_endpoint: album={album:?}");

    let path = get_album_cover(&**api, &**data.database, &album, size).await?;
    let path_buf = std::path::PathBuf::from(path);
    let file_path = path_buf.as_path();

    let file = actix_files::NamedFile::open_async(file_path)
        .await
        .map_err(|e| AlbumCoverError::File(file_path.to_str().unwrap().into(), format!("{e:?}")))
        .map_err(|e| ErrorInternalServerError(e.to_string()))?;

    Ok(file.into_response(&req))
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Files", "head"],
        get,
        path = "/albums/{artistId}/{size}",
        description = "Get album cover at the specified dimensions",
        params(
            ("artistId" = u64, Path, description = "The Artist ID"),
            ("size" = String, Path, description = "The {width}x{height} of the image"),
        ),
        responses(
            (
                status = 200,
                description = "The album cover at the specified dimensions",
            )
        )
    )
)]
#[route("/albums/{album_id}/{size}", method = "GET", method = "HEAD")]
pub async fn album_artwork_endpoint(
    path: web::Path<(String, String)>,
    query: web::Query<AlbumCoverQuery>,
    data: web::Data<AppState>,
    api_state: web::Data<MusicApiState>,
) -> Result<HttpResponse> {
    let paths = path.into_inner();

    let album_id_string = paths.0;
    let source = query.source.unwrap_or(ApiSource::Library);
    let album_id = match source {
        ApiSource::Library => album_id_string.parse::<u64>().map(Id::Number),
        ApiSource::Tidal => album_id_string.parse::<u64>().map(Id::Number),
        ApiSource::Qobuz => Ok(Id::String(album_id_string)),
        ApiSource::Yt => Ok(Id::String(album_id_string)),
    }
    .map_err(|_e| ErrorBadRequest("Invalid album_id"))?;

    let dimensions = paths
        .1
        .split('x')
        .take(2)
        .map(|dimension| {
            dimension
                .parse::<u32>()
                .map_err(|_e| ErrorBadRequest("Invalid dimension"))
        })
        .collect::<Result<Vec<_>>>()?;
    let (width, height) = (dimensions[0], dimensions[1]);

    let size = (std::cmp::max(width, height) as u16).into();
    let api = api_state
        .apis
        .get(source)
        .map_err(|e| ErrorInternalServerError(format!("Failed to get music_api: {e:?}")))?;
    let album = api
        .album(&album_id)
        .await
        .map_err(|e| ErrorNotFound(format!("Failed to get album: {e:?}")))?
        .ok_or_else(|| ErrorNotFound(format!("Album not found: {}", album_id.to_owned())))?;

    log::debug!("album_cover_endpoint: album={album:?}");

    let path = get_album_cover(&**api, &**data.database, &album, size).await?;

    resize_image_path(&path, width, height)
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to resize image: {e:?}")))
}
