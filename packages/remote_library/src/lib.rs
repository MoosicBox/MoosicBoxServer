#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use async_trait::async_trait;
use moosicbox_core::{
    sqlite::models::{Album, ApiSource, Artist, Id, Track},
    types::PlaybackQuality,
};
use moosicbox_music_api::{
    AddAlbumError, AddArtistError, AddTrackError, AlbumError, AlbumOrder, AlbumOrderDirection,
    AlbumType, AlbumsError, AlbumsRequest, ArtistAlbumsError, ArtistError, ArtistOrder,
    ArtistOrderDirection, ArtistsError, ImageCoverSize, ImageCoverSource, MusicApi,
    RemoveAlbumError, RemoveArtistError, RemoveTrackError, TrackAudioQuality, TrackError,
    TrackOrId, TrackOrder, TrackOrderDirection, TrackSource, TracksError,
};
use moosicbox_paging::PagingResult;
use reqwest::Client;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RequestError {
    #[error(transparent)]
    Request(#[from] reqwest::Error),
    #[error("Unsuccessful: {0}")]
    Unsuccessful(String),
}

#[derive(Clone)]
pub struct RemoteLibraryMusicApi {
    client: Client,
    host: String,
    api_source: ApiSource,
}

impl RemoteLibraryMusicApi {
    pub fn new(host: String, api_source: ApiSource) -> Self {
        let client = Client::new();

        Self {
            client,
            host,
            api_source,
        }
    }
}

#[async_trait]
impl MusicApi for RemoteLibraryMusicApi {
    fn source(&self) -> ApiSource {
        unimplemented!("Dynamic MusicApi must be implemented by the struct")
    }

    async fn artists(
        &self,
        _offset: Option<u32>,
        _limit: Option<u32>,
        _order: Option<ArtistOrder>,
        _order_direction: Option<ArtistOrderDirection>,
    ) -> PagingResult<Artist, ArtistsError> {
        unimplemented!("Fetching artists is not implemented")
    }

    async fn artist(&self, _artist_id: &Id) -> Result<Option<Artist>, ArtistError> {
        unimplemented!("Fetching artist is not implemented")
    }

    async fn add_artist(&self, _artist_id: &Id) -> Result<(), AddArtistError> {
        unimplemented!("Adding artist is not implemented")
    }

    async fn remove_artist(&self, _artist_id: &Id) -> Result<(), RemoveArtistError> {
        unimplemented!("Removing artist is not implemented")
    }

    async fn album_artist(&self, _album_id: &Id) -> Result<Option<Artist>, ArtistError> {
        unimplemented!("Fetching album artist is not implemented")
    }

    async fn artist_cover_source(
        &self,
        _artist: &Artist,
        _size: ImageCoverSize,
    ) -> Result<Option<ImageCoverSource>, ArtistError> {
        unimplemented!("Fetching artist cover source is not implemented")
    }

    async fn albums(&self, _request: &AlbumsRequest) -> PagingResult<Album, AlbumsError> {
        unimplemented!("Fetching albums is not implemented")
    }

    async fn album(&self, _album_id: &Id) -> Result<Option<Album>, AlbumError> {
        unimplemented!("Fetching album is not implemented")
    }

    async fn artist_albums(
        &self,
        _artist_id: &Id,
        _album_type: AlbumType,
        _offset: Option<u32>,
        _limit: Option<u32>,
        _order: Option<AlbumOrder>,
        _order_direction: Option<AlbumOrderDirection>,
    ) -> PagingResult<Album, ArtistAlbumsError> {
        unimplemented!("Fetching artist albums is not implemented")
    }

    async fn add_album(&self, _album_id: &Id) -> Result<(), AddAlbumError> {
        unimplemented!("Adding album is not implemented")
    }

    async fn remove_album(&self, _album_id: &Id) -> Result<(), RemoveAlbumError> {
        unimplemented!("Removing album is not implemented")
    }

    async fn album_cover_source(
        &self,
        _album: &Album,
        _size: ImageCoverSize,
    ) -> Result<Option<ImageCoverSource>, AlbumError> {
        unimplemented!("Fetching album cover source is not implemented")
    }

    async fn tracks(
        &self,
        _track_ids: Option<&[Id]>,
        _offset: Option<u32>,
        _limit: Option<u32>,
        _order: Option<TrackOrder>,
        _order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, TracksError> {
        unimplemented!("Fetching tracks is not implemented")
    }

    async fn album_tracks(
        &self,
        _album_id: &Id,
        _offset: Option<u32>,
        _limit: Option<u32>,
        _order: Option<TrackOrder>,
        _order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, TracksError> {
        unimplemented!("Fetching album tracks is not implemented")
    }

    async fn track(&self, track_id: &Id) -> Result<Option<Track>, TrackError> {
        let request = self.client.request(
            reqwest::Method::GET,
            format!(
                "{host}/menu/track?trackId={track_id}&source={source}",
                host = self.host,
                source = self.api_source
            ),
        );

        let response = request
            .send()
            .await
            .map_err(|e| TrackError::Other(Box::new(e)))?;

        if !response.status().is_success() {
            if response.status() == 404 {
                return Ok(None);
            }
            return Err(TrackError::Other(Box::new(RequestError::Unsuccessful(
                format!("Status {}", response.status()),
            ))));
        }

        let value = response
            .json()
            .await
            .map_err(|e| TrackError::Other(Box::new(e)))?;

        Ok(Some(value))
    }

    async fn add_track(&self, _track_id: &Id) -> Result<(), AddTrackError> {
        unimplemented!("Adding track is not implemented")
    }

    async fn remove_track(&self, _track_id: &Id) -> Result<(), RemoveTrackError> {
        unimplemented!("Removing track is not implemented")
    }

    async fn track_source(
        &self,
        _track: TrackOrId,
        _quality: TrackAudioQuality,
    ) -> Result<Option<TrackSource>, TrackError> {
        unimplemented!("Fetching track source is not implemented")
    }

    async fn track_size(
        &self,
        _track: TrackOrId,
        _source: &TrackSource,
        _quality: PlaybackQuality,
    ) -> Result<Option<u64>, TrackError> {
        unimplemented!("Fetching track size is not implemented")
    }
}
