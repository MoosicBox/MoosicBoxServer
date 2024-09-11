use std::collections::HashMap;
use std::sync::LazyLock;

use moosicbox_async_service::Arc;
use moosicbox_audio_output::AudioOutputScannerError;
use moosicbox_core::sqlite::models::ApiSource;
use moosicbox_database::Database;
use moosicbox_ws::WebsocketSendError;
use thiserror::Error;

use crate::{DB, WS_SERVER_HANDLE};

pub static SERVER_PLAYERS: LazyLock<
    tokio::sync::RwLock<
        HashMap<
            u64,
            (
                moosicbox_player::local::LocalPlayer,
                moosicbox_player::PlaybackHandler,
            ),
        >,
    >,
> = LazyLock::new(|| tokio::sync::RwLock::new(HashMap::new()));

#[derive(Debug, Error)]
pub enum InitError {
    #[error(transparent)]
    WebsocketSend(#[from] WebsocketSendError),
    #[error(transparent)]
    AudioOutputScanner(#[from] AudioOutputScannerError),
}

pub async fn init(
    db: Arc<Box<dyn Database>>,
    tunnel_handle: Option<moosicbox_tunnel_sender::sender::TunnelSenderHandle>,
) -> Result<(), InitError> {
    moosicbox_audio_output::scan_outputs().await?;

    let handle =
        WS_SERVER_HANDLE
            .read()
            .await
            .clone()
            .ok_or(moosicbox_ws::WebsocketSendError::Unknown(
                "No ws server handle".into(),
            ))?;

    for audio_output in moosicbox_audio_output::output_factories().await {
        if let Err(err) =
            register_server_player(&**db, handle.clone(), &tunnel_handle, audio_output.clone())
                .await
        {
            log::error!("Failed to register server player: {err:?}");
        } else {
            log::debug!("Registered server player audio_output={audio_output:?}");
        }
    }

    Ok(())
}

#[allow(clippy::too_many_lines)]
fn handle_server_playback_update(
    update: &moosicbox_session::models::UpdateSession,
) -> std::pin::Pin<Box<dyn futures_util::Future<Output = ()> + Send>> {
    use moosicbox_core::sqlite::models::Id;
    use moosicbox_player::PlaybackHandler;
    use moosicbox_session::get_session;

    let update = update.clone();
    let db = DB.read().unwrap().clone().unwrap().clone();

    Box::pin(async move {
        log::debug!("Handling server playback update");

        let update = update;

        let updated = {
            {
                let audio_zone =
                    match moosicbox_session::get_session_audio_zone(&**db, update.session_id).await
                    {
                        Ok(players) => players,
                        Err(e) => moosicbox_assert::die_or_panic!(
                            "Failed to get session active players: {e:?}"
                        ),
                    };

                let Some(audio_zone) = audio_zone else {
                    return;
                };

                let existing = { SERVER_PLAYERS.read().await.get(&update.session_id).cloned() };
                let existing = existing.filter(|(player, _)| {
                    player.output.as_ref().is_some_and(|output| {
                        !audio_zone
                            .players
                            .iter()
                            .any(|p| p.audio_output_id != output.lock().unwrap().id)
                    })
                });

                if let Some((_, player)) = existing {
                    player
                } else {
                    let outputs = moosicbox_audio_output::output_factories().await;

                    // TODO: handle more than one output
                    let output = audio_zone
                        .players
                        .into_iter()
                        .find_map(|x| outputs.iter().find(|output| output.id == x.audio_output_id))
                        .cloned();

                    let Some(output) = output else {
                        moosicbox_assert::die_or_panic!("No output available");
                    };

                    let mut players = SERVER_PLAYERS.write().await;

                    let db = { DB.read().unwrap().clone().expect("No database") };

                    let local_player = match moosicbox_player::local::LocalPlayer::new(
                        moosicbox_player::PlayerSource::Local,
                        None,
                    )
                    .await
                    {
                        Ok(player) => player,
                        Err(e) => {
                            moosicbox_assert::die_or_panic!("Failed to create new player: {e:?}")
                        }
                    }
                    .with_output(output);

                    let playback = local_player.playback.clone();
                    let output = local_player.output.clone();
                    let receiver = local_player.receiver.clone();

                    let mut player = PlaybackHandler::new(local_player.clone())
                        .with_playback(playback)
                        .with_output(output)
                        .with_receiver(receiver);

                    local_player
                        .playback_handler
                        .write()
                        .unwrap()
                        .replace(player.clone());

                    if let Ok(Some(session)) = get_session(&**db, update.session_id).await {
                        if let Err(e) = player.init_from_session(session, &update).await {
                            moosicbox_assert::die_or_error!(
                                "Failed to create new player from session: {e:?}"
                            );
                        }
                    }

                    players.insert(update.session_id, (local_player, player.clone()));

                    player
                }
            }
            .update_playback(
                true,
                update.play,
                update.stop,
                update.playing,
                update.position,
                update.seek,
                update.volume,
                if let Some(playlist) = update.playlist {
                    let track_ids = playlist
                        .tracks
                        .iter()
                        .filter_map(|x| x.id.parse::<u64>().ok())
                        .map(std::convert::Into::into)
                        .collect::<Vec<Id>>();

                    let tracks = match moosicbox_library::db::get_tracks(&**db, Some(&track_ids))
                        .await
                    {
                        Ok(tracks) => tracks,
                        Err(e) => moosicbox_assert::die_or_panic!("Failed to get tracks: {e:?}"),
                    };

                    Some(
                        playlist
                            .tracks
                            .iter()
                            .map(|x| {
                                let data = if x.r#type == ApiSource::Library {
                                    tracks
                                        .iter()
                                        .find(|track| x.id == track.id.to_string())
                                        .and_then(|x| serde_json::to_value(x).ok())
                                } else {
                                    x.data.clone().and_then(|x| serde_json::to_value(x).ok())
                                };

                                moosicbox_player::Track {
                                    id: x.id.clone().into(),
                                    source: x.r#type,
                                    data,
                                }
                            })
                            .collect::<Vec<_>>(),
                    )
                } else {
                    None
                },
                None,
                Some(update.session_id),
                Some(update.playback_target),
                false,
                Some(moosicbox_player::DEFAULT_PLAYBACK_RETRY_OPTIONS),
            )
            .await
        };

        match updated {
            Ok(status) => {
                log::debug!("Updated server player playback: {status:?}");
            }
            Err(err) => {
                log::error!("Failed to update server player playback: {err:?}");
            }
        }
    })
}

pub async fn register_server_player(
    db: &dyn Database,
    ws: crate::ws::server::WsServerHandle,
    tunnel_handle: &Option<moosicbox_tunnel_sender::sender::TunnelSenderHandle>,
    audio_output: moosicbox_audio_output::AudioOutputFactory,
) -> Result<(), moosicbox_ws::WebsocketSendError> {
    use crate::WS_SERVER_HANDLE;

    let connection_id = "self";

    let context = moosicbox_ws::WebsocketContext {
        connection_id: connection_id.to_string(),
        ..Default::default()
    };
    let payload = moosicbox_session::models::RegisterConnection {
        connection_id: connection_id.to_string(),
        name: "MoosicBox Server".to_string(),
        players: vec![moosicbox_session::models::RegisterPlayer {
            name: audio_output.name,
            audio_output_id: audio_output.id.clone(),
        }],
    };

    let handle =
        WS_SERVER_HANDLE
            .read()
            .await
            .clone()
            .ok_or(moosicbox_ws::WebsocketSendError::Unknown(
                "No ws server handle".into(),
            ))?;

    let connection = moosicbox_ws::register_connection(db, &handle, &context, &payload).await?;

    let player = connection
        .players
        .iter()
        .find(|x| x.audio_output_id == audio_output.id)
        .ok_or(moosicbox_ws::WebsocketSendError::Unknown(
            "No player on connection".into(),
        ))?;

    ws.add_player_action(player.id, handle_server_playback_update)
        .await;

    if let Some(handle) = tunnel_handle {
        handle.add_player_action(player.id, handle_server_playback_update);
    }

    moosicbox_ws::get_sessions(db, &handle, &context, true).await
}