#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

mod api;
#[cfg(feature = "static-token-auth")]
mod auth;
mod playback_session;
mod ws;

use actix_cors::Cors;
use actix_web::{http, middleware, web, App};
use log::{debug, error, info};
use moosicbox_auth::get_client_id_and_access_token;
use moosicbox_core::app::AppState;
use moosicbox_database::{rusqlite::RusqliteDatabase, Database, DbConnection};
use moosicbox_downloader::{api::models::ApiProgressEvent, queue::ProgressEvent};
use moosicbox_env_utils::{default_env, default_env_usize, option_env_usize};
use moosicbox_tunnel::TunnelRequest;
use moosicbox_tunnel_sender::{sender::TunnelSender, TunnelMessage};
use moosicbox_ws::api::send_download_event;
use once_cell::sync::Lazy;
use std::{
    env,
    sync::{atomic::AtomicUsize, Arc, Mutex, OnceLock},
    time::Duration,
};
use throttle::Throttle;
use tokio::{task::spawn, try_join};
use tokio_util::sync::CancellationToken;
use url::Url;
use ws::server::ChatServer;

static CANCELLATION_TOKEN: Lazy<CancellationToken> = Lazy::new(CancellationToken::new);

static CHAT_SERVER_HANDLE: Lazy<std::sync::RwLock<Option<ws::server::ChatServerHandle>>> =
    Lazy::new(|| std::sync::RwLock::new(None));

static DB: OnceLock<Arc<Box<dyn Database>>> = OnceLock::new();

fn main() -> std::io::Result<()> {
    env_logger::init();

    let args: Vec<String> = env::args().collect();

    let service_port = if args.len() > 1 {
        args[1].parse::<u16>().expect("Invalid port argument")
    } else {
        default_env_usize("PORT", 8000)
            .unwrap_or(8000)
            .try_into()
            .expect("Invalid PORT environment variable")
    };

    moosicbox_player::player::set_service_port(service_port);
    moosicbox_player::player::on_playback_event(crate::playback_session::on_playback_event);

    actix_web::rt::System::with_tokio_rt(|| {
        let threads = default_env_usize("MAX_THREADS", 64).unwrap_or(64);
        log::debug!("Running with {threads} max blocking threads");
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .max_blocking_threads(threads)
            .build()
            .unwrap()
    })
    .block_on(async move {
        let library = ::rusqlite::Connection::open("library.db").unwrap();
        library
            .busy_timeout(Duration::from_millis(10))
            .expect("Failed to set busy timeout");
        let library = Arc::new(Mutex::new(DbConnection { inner: library }));

        let db = Box::new(RusqliteDatabase::new(library));
        let database: Arc<Box<dyn Database>> = Arc::new(db);
        let database_once: Arc<Box<dyn Database>> = database.clone();

        let db = DB.get_or_init(move || database_once.clone());

        let bytes_throttle = Arc::new(Mutex::new(Throttle::new(Duration::from_millis(200), 1)));
        let bytes_buf = AtomicUsize::new(0);

        moosicbox_downloader::api::add_progress_listener_to_download_queue(Box::new(
            move |event| {
                let binding = CHAT_SERVER_HANDLE.read().unwrap_or_else(|e| e.into_inner());
                let sender = binding.as_ref().unwrap();

                let event = if let ProgressEvent::BytesRead { task, read, total } = event {
                    if let Err(_) = bytes_throttle.lock().unwrap().accept() {
                        bytes_buf.fetch_add(*read, std::sync::atomic::Ordering::SeqCst);
                        return;
                    } else {
                        let bytes = bytes_buf.load(std::sync::atomic::Ordering::SeqCst);
                        bytes_buf.store(0, std::sync::atomic::Ordering::SeqCst);
                        ProgressEvent::BytesRead {
                            task: task.clone(),
                            read: *read + bytes,
                            total: *total,
                        }
                    }
                } else {
                    event.clone()
                };

                let api_event: ApiProgressEvent = event.into();

                if let Err(err) = send_download_event(sender, None, api_event) {
                    log::error!("Failed to broadcast download event: {err:?}");
                }
            },
        ))
        .await;

        let (mut chat_server, server_tx) = ChatServer::new(database.clone());
        CHAT_SERVER_HANDLE.write().unwrap().replace(server_tx);

        let (tunnel_host, tunnel_join_handle, tunnel_handle) = if let Ok(url) = env::var("WS_HOST")
        {
            let ws_url = url.clone();
            let url = Url::parse(&url).expect("Invalid WS_HOST");
            let hostname = url
                .host_str()
                .map(|s| s.to_string())
                .expect("Invalid WS_HOST");
            let host = format!(
                "{}://{hostname}{}",
                if url.scheme() == "wss" {
                    "https"
                } else {
                    "http"
                },
                if let Some(port) = url.port() {
                    format!(":{port}")
                } else {
                    "".to_string()
                }
            );
            let (client_id, access_token) = {
                get_client_id_and_access_token(&db, &host)
                    .await
                    .map_err(|e| {
                        std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!("Could not get access token: {e:?}"),
                        )
                    })?
            };
            let (mut tunnel, handle) =
                TunnelSender::new(host.clone(), ws_url, client_id, access_token);

            let database_send = database.clone();
            (
                Some(host),
                Some(spawn(async move {
                    let mut rx = tunnel.start();

                    while let Some(m) = rx.recv().await {
                        match m {
                            TunnelMessage::Text(m) => {
                                debug!("Received text TunnelMessage {}", &m);
                                let tunnel = tunnel.clone();
                                let database_send = database_send.clone();
                                spawn(async move {
                                    match serde_json::from_str(&m).unwrap() {
                                        TunnelRequest::Http(request) => {
                                            if let Err(err) = tunnel
                                                .tunnel_request(
                                                    database_send.clone(),
                                                    service_port,
                                                    request.request_id,
                                                    request.method,
                                                    request.path,
                                                    request.query,
                                                    request.payload,
                                                    request.headers,
                                                    request.encoding,
                                                )
                                                .await
                                            {
                                                log::error!("Tunnel request failed: {err:?}");
                                            }
                                        }
                                        TunnelRequest::Ws(request) => {
                                            let sender = CHAT_SERVER_HANDLE
                                                .read()
                                                .unwrap_or_else(|e| e.into_inner())
                                                .as_ref()
                                                .ok_or("Failed to get chat server handle")?
                                                .clone();
                                            if let Err(err) = tunnel
                                                .ws_request(
                                                    &database_send,
                                                    request.request_id,
                                                    request.body.clone(),
                                                    sender,
                                                )
                                                .await
                                            {
                                                error!(
                                                    "Failed to propagate ws request {}: {err:?}",
                                                    request.request_id
                                                );
                                            }
                                        }
                                        TunnelRequest::Abort(request) => {
                                            log::debug!("Aborting request {}", request.request_id);
                                            tunnel.abort_request(request.request_id);
                                        }
                                    }
                                    Ok::<_, String>(())
                                });
                            }
                            TunnelMessage::Binary(_) => todo!(),
                            TunnelMessage::Ping(_) => {}
                            TunnelMessage::Pong(_) => todo!(),
                            TunnelMessage::Close => {
                                info!("Tunnel connection was closed");
                            }
                            TunnelMessage::Frame(_) => todo!(),
                        }
                    }
                    debug!("Exiting tunnel message loop");
                })),
                Some(handle),
            )
        } else {
            (None, None, None)
        };

        if let Some(ref tunnel_handle) = tunnel_handle {
            chat_server.add_sender(Box::new(tunnel_handle.clone()));
        }

        let chat_server_handle = spawn(async move { chat_server.run().await });

        let app = move || {
            let app_data = AppState {
                tunnel_host: tunnel_host.clone(),
                service_port,
                database: database.clone(),
            };

            let cors = Cors::default()
                .allow_any_origin()
                .allowed_methods(vec!["GET", "POST", "OPTIONS", "DELETE", "PUT", "PATCH"])
                .allowed_headers(vec![http::header::AUTHORIZATION, http::header::ACCEPT])
                .allowed_header(http::header::CONTENT_TYPE)
                .supports_credentials()
                .max_age(3600);

            let app = App::new().wrap(cors).wrap(middleware::Compress::default());

            #[cfg(feature = "static-token-auth")]
            let app = app.wrap(crate::auth::StaticTokenAuth::new(
                std::env!("STATIC_TOKEN").into(),
            ));

            app.app_data(web::Data::new(app_data))
                .service(api::health_endpoint)
                .service(api::websocket)
                .service(moosicbox_scan::api::run_scan_endpoint)
                .service(moosicbox_scan::api::run_scan_path_endpoint)
                .service(moosicbox_scan::api::get_scan_origins_endpoint)
                .service(moosicbox_scan::api::enable_scan_origin_endpoint)
                .service(moosicbox_scan::api::disable_scan_origin_endpoint)
                .service(moosicbox_scan::api::get_scan_paths_endpoint)
                .service(moosicbox_scan::api::add_scan_path_endpoint)
                .service(moosicbox_scan::api::remove_scan_path_endpoint)
                .service(moosicbox_auth::api::get_magic_token_endpoint)
                .service(moosicbox_auth::api::create_magic_token_endpoint)
                .service(moosicbox_downloader::api::download_endpoint)
                .service(moosicbox_downloader::api::download_tasks_endpoint)
                .service(moosicbox_menu::api::get_artists_endpoint)
                .service(moosicbox_menu::api::get_artist_endpoint)
                .service(moosicbox_menu::api::get_album_endpoint)
                .service(moosicbox_menu::api::add_album_endpoint)
                .service(moosicbox_menu::api::remove_album_endpoint)
                .service(moosicbox_menu::api::refavorite_album_endpoint)
                .service(moosicbox_menu::api::get_albums_endpoint)
                .service(moosicbox_menu::api::get_tracks_endpoint)
                .service(moosicbox_menu::api::get_album_tracks_endpoint)
                .service(moosicbox_menu::api::get_album_versions_endpoint)
                .service(moosicbox_menu::api::get_artist_albums_endpoint)
                .service(moosicbox_files::api::track_endpoint)
                .service(moosicbox_files::api::track_visualization_endpoint)
                .service(moosicbox_files::api::track_info_endpoint)
                .service(moosicbox_files::api::tracks_info_endpoint)
                .service(moosicbox_files::api::artist_cover_endpoint)
                .service(moosicbox_files::api::album_source_artwork_endpoint)
                .service(moosicbox_files::api::album_artwork_endpoint)
                .service(moosicbox_player::api::play_track_endpoint)
                .service(moosicbox_player::api::play_tracks_endpoint)
                .service(moosicbox_player::api::play_album_endpoint)
                .service(moosicbox_player::api::pause_playback_endpoint)
                .service(moosicbox_player::api::resume_playback_endpoint)
                .service(moosicbox_player::api::update_playback_endpoint)
                .service(moosicbox_player::api::next_track_endpoint)
                .service(moosicbox_player::api::previous_track_endpoint)
                .service(moosicbox_player::api::stop_track_endpoint)
                .service(moosicbox_player::api::seek_track_endpoint)
                .service(moosicbox_player::api::player_status_endpoint)
                .service(moosicbox_search::api::reindex_endpoint)
                .service(moosicbox_search::api::search_global_search_endpoint)
                .service(moosicbox_tidal::api::device_authorization_endpoint)
                .service(moosicbox_tidal::api::device_authorization_token_endpoint)
                .service(moosicbox_tidal::api::track_file_url_endpoint)
                .service(moosicbox_tidal::api::track_playback_info_endpoint)
                .service(moosicbox_tidal::api::favorite_artists_endpoint)
                .service(moosicbox_tidal::api::add_favorite_artist_endpoint)
                .service(moosicbox_tidal::api::remove_favorite_artist_endpoint)
                .service(moosicbox_tidal::api::favorite_albums_endpoint)
                .service(moosicbox_tidal::api::add_favorite_album_endpoint)
                .service(moosicbox_tidal::api::remove_favorite_album_endpoint)
                .service(moosicbox_tidal::api::favorite_tracks_endpoint)
                .service(moosicbox_tidal::api::add_favorite_track_endpoint)
                .service(moosicbox_tidal::api::remove_favorite_track_endpoint)
                .service(moosicbox_tidal::api::artist_albums_endpoint)
                .service(moosicbox_tidal::api::album_tracks_endpoint)
                .service(moosicbox_tidal::api::album_endpoint)
                .service(moosicbox_tidal::api::artist_endpoint)
                .service(moosicbox_tidal::api::track_endpoint)
                .service(moosicbox_qobuz::api::user_login_endpoint)
                .service(moosicbox_qobuz::api::track_file_url_endpoint)
                .service(moosicbox_qobuz::api::favorite_artists_endpoint)
                .service(moosicbox_qobuz::api::favorite_albums_endpoint)
                .service(moosicbox_qobuz::api::favorite_tracks_endpoint)
                .service(moosicbox_qobuz::api::artist_albums_endpoint)
                .service(moosicbox_qobuz::api::album_tracks_endpoint)
                .service(moosicbox_qobuz::api::album_endpoint)
                .service(moosicbox_qobuz::api::artist_endpoint)
                .service(moosicbox_qobuz::api::track_endpoint)
        };

        let mut http_server = actix_web::HttpServer::new(app);

        if let Ok(Some(workers)) = option_env_usize("ACTIX_WORKERS") {
            log::debug!("Running with {workers} Actix workers");
            http_server = http_server.workers(workers);
        }

        let http_server = http_server
            .bind((default_env("BIND_ADDR", "0.0.0.0"), service_port))?
            .run();

        try_join!(
            async move {
                let resp = http_server.await;
                CHAT_SERVER_HANDLE.write().unwrap().take();
                moosicbox_scan::cancel();
                CANCELLATION_TOKEN.cancel();
                if let Some(handle) = tunnel_handle {
                    let _ = handle.close().await;
                }
                resp
            },
            async move { chat_server_handle.await.unwrap() },
            async move {
                if let Some(handle) = tunnel_join_handle {
                    handle.await.unwrap();
                }
                Ok(())
            }
        )?;

        Ok(())
    })
}