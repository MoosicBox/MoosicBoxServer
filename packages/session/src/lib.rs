#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use models::{
    CreateSession, Session, SessionPlaylist, SessionPlaylistTrack, SetSessionAudioZone,
    UpdateSession,
};
use moosicbox_audio_zone::models::{AudioZone, Player};
use moosicbox_core::sqlite::db::DbError;
use moosicbox_database::{config::ConfigDatabase, profiles::LibraryDatabase, TryIntoDb};

mod db;
pub mod models;

#[cfg(feature = "api")]
pub mod api;

#[cfg(feature = "events")]
pub mod events;

pub async fn get_session_playlist_tracks(
    db: &LibraryDatabase,
    session_playlist_id: u64,
) -> Result<Vec<SessionPlaylistTrack>, DbError> {
    crate::db::get_session_playlist_tracks(db, session_playlist_id).await
}

pub async fn get_session_playlist(
    db: &LibraryDatabase,
    session_id: u64,
) -> Result<Option<SessionPlaylist>, DbError> {
    crate::db::get_session_playlist(db, session_id).await
}

pub async fn get_session_audio_zone(
    db: &LibraryDatabase,
    session_id: u64,
) -> Result<Option<AudioZone>, DbError> {
    Ok(crate::db::get_session_audio_zone(db, session_id)
        .await?
        .try_into_db(db.into())
        .await?)
}

pub async fn set_session_audio_zone(
    db: &LibraryDatabase,
    set_session_audio_zone: &SetSessionAudioZone,
) -> Result<(), DbError> {
    crate::db::set_session_audio_zone(db, set_session_audio_zone).await
}

pub async fn get_session_playing(db: &LibraryDatabase, id: u64) -> Result<Option<bool>, DbError> {
    crate::db::get_session_playing(db, id).await
}

pub async fn get_session(db: &LibraryDatabase, id: u64) -> Result<Option<Session>, DbError> {
    crate::db::get_session(db, id).await
}

pub async fn get_sessions(db: &LibraryDatabase) -> Result<Vec<Session>, DbError> {
    crate::db::get_sessions(db).await
}

pub async fn create_session(
    db: &LibraryDatabase,
    session: &CreateSession,
) -> Result<Session, DbError> {
    crate::db::create_session(db, session).await
}

pub async fn update_session(db: &LibraryDatabase, session: &UpdateSession) -> Result<(), DbError> {
    crate::db::update_session(db, session).await
}

pub async fn delete_session(db: &LibraryDatabase, session_id: u64) -> Result<(), DbError> {
    crate::db::delete_session(db, session_id).await
}

pub async fn get_connections(db: &ConfigDatabase) -> Result<Vec<models::Connection>, DbError> {
    crate::db::get_connections(db).await
}

pub async fn register_connection(
    db: &ConfigDatabase,
    connection: &models::RegisterConnection,
) -> Result<models::Connection, DbError> {
    let result = crate::db::register_connection(db, connection).await?;

    for player in &connection.players {
        create_player(db, &connection.connection_id, player).await?;
    }

    let players = get_players(db, &result.id).await?;

    Ok(models::Connection {
        id: result.id,
        name: result.name,
        created: result.created,
        updated: result.updated,
        players,
    })
}

pub async fn delete_connection(db: &ConfigDatabase, connection_id: &str) -> Result<(), DbError> {
    crate::db::delete_connection(db, connection_id).await
}

pub async fn get_players(db: &ConfigDatabase, connection_id: &str) -> Result<Vec<Player>, DbError> {
    crate::db::get_players(db, connection_id).await
}

pub async fn create_player(
    db: &ConfigDatabase,
    connection_id: &str,
    player: &models::RegisterPlayer,
) -> Result<Player, DbError> {
    let result = crate::db::create_player(db, connection_id, player).await?;

    #[cfg(feature = "events")]
    {
        moosicbox_task::spawn("create_player updated_events", async move {
            if let Err(e) = crate::events::trigger_players_updated_event().await {
                moosicbox_assert::die_or_error!("Failed to trigger event: {e:?}");
            }
        });
    }

    Ok(result)
}

pub async fn create_players(
    db: &ConfigDatabase,
    connection_id: &str,
    players: &[models::RegisterPlayer],
) -> Result<Vec<Player>, DbError> {
    let mut results = vec![];

    for player in players {
        results.push(crate::db::create_player(db, connection_id, player).await?);
    }

    #[cfg(feature = "events")]
    {
        moosicbox_task::spawn("create_players updated_events", async move {
            if let Err(e) = crate::events::trigger_players_updated_event().await {
                moosicbox_assert::die_or_error!("Failed to trigger event: {e:?}");
            }
        });
    }

    Ok(results)
}

pub async fn delete_player(db: &ConfigDatabase, player_id: u64) -> Result<(), DbError> {
    crate::db::delete_player(db, player_id).await?;

    #[cfg(feature = "events")]
    {
        moosicbox_task::spawn("delete_player updated_events", async move {
            if let Err(e) = crate::events::trigger_players_updated_event().await {
                moosicbox_assert::die_or_error!("Failed to trigger event: {e:?}");
            }
        });
    }

    Ok(())
}

pub async fn delete_session_playlist_track_by_track_id(
    db: &LibraryDatabase,
    id: u64,
) -> Result<Option<SessionPlaylistTrack>, DbError> {
    crate::db::delete_session_playlist_track_by_track_id(db, id).await
}

pub async fn delete_session_playlist_tracks_by_track_id(
    db: &LibraryDatabase,
    ids: Option<&Vec<u64>>,
) -> Result<Vec<SessionPlaylistTrack>, DbError> {
    crate::db::delete_session_playlist_tracks_by_track_id(db, ids).await
}
