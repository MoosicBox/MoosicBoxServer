//! A multi-room chat server.

use std::{
    collections::HashMap,
    fmt, io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use moosicbox_tunnel::{
    TunnelAbortRequest, TunnelRequest, TunnelResponse, TunnelWsRequest, TunnelWsResponse,
};
use rand::{thread_rng, Rng as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::EnumString;
use thiserror::Error;
use tokio::sync::{
    mpsc::{self, error::SendError, UnboundedSender},
    oneshot, RwLock,
};
use tokio_util::sync::CancellationToken;

use crate::db::{delete_connection, select_connection, upsert_connection, DatabaseError};
use crate::ws::{ConnId, Msg};

/// A command received by the [`ChatServer`].
#[derive(Debug)]
enum Command {
    Connect {
        conn_tx: mpsc::UnboundedSender<Msg>,
        res_tx: oneshot::Sender<ConnId>,
        client_id: String,
        sender: bool,
    },

    Disconnect {
        conn: ConnId,
    },

    RequestStart {
        request_id: usize,
        sender: UnboundedSender<TunnelResponse>,
        headers_sender: oneshot::Sender<RequestHeaders>,
        abort_request_token: CancellationToken,
    },

    RequestEnd {
        request_id: usize,
    },

    Response {
        response: TunnelResponse,
        conn_id: ConnId,
    },

    WsRequest {
        request_id: usize,
        conn_id: ConnId,
        client_id: String,
        body: String,
    },

    WsMessage {
        message: TunnelWsResponse,
    },

    WsResponse {
        response: TunnelWsResponse,
    },

    Message {
        msg: Msg,
        conn: ConnId,
    },
}

#[derive(Debug)]
pub struct RequestHeaders {
    pub status: u16,
    pub headers: HashMap<String, String>,
}

/// A multi-room chat server.
///
/// Contains the logic of how connections chat with each other plus room management.
///
/// Call and spawn [`run`](Self::run) to start processing commands.
#[derive(Debug)]
pub struct ChatServer {
    /// Map of connection IDs to their message receivers.
    sessions: HashMap<ConnId, mpsc::UnboundedSender<Msg>>,
    clients: HashMap<ConnId, mpsc::UnboundedSender<Msg>>,
    senders: HashMap<usize, UnboundedSender<TunnelResponse>>,
    headers_senders: HashMap<usize, oneshot::Sender<RequestHeaders>>,
    abort_request_tokens: HashMap<usize, CancellationToken>,

    /// Tracks total number of historical connections established.
    visitor_count: Arc<AtomicUsize>,

    /// Command receiver.
    cmd_rx: flume::Receiver<Command>,

    ws_requests: HashMap<usize, ConnId>,
}

#[derive(Debug, Serialize, Deserialize, EnumString)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum InboundMessageType {
    Ping,
}

impl fmt::Display for InboundMessageType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Error)]
pub enum WebsocketMessageError {
    #[error("Session {0} not connected")]
    NoSession(ConnId),
    #[error(transparent)]
    WebsocketSend(#[from] SendError<String>),
}

impl ChatServer {
    pub fn new() -> (Self, ChatServerHandle) {
        let (cmd_tx, cmd_rx) = flume::unbounded();

        (
            Self {
                sessions: HashMap::new(),
                clients: HashMap::new(),
                senders: HashMap::new(),
                headers_senders: HashMap::new(),
                abort_request_tokens: HashMap::new(),
                visitor_count: Arc::new(AtomicUsize::new(0)),
                cmd_rx,
                ws_requests: HashMap::new(),
            },
            ChatServerHandle { cmd_tx },
        )
    }

    async fn abort_request(
        &self,
        id: ConnId,
        request_id: usize,
    ) -> Result<(), WebsocketMessageError> {
        log::debug!("Aborting request {request_id} (conn_id={id})");
        if let Some(abort_token) = self.abort_request_tokens.get(&request_id) {
            abort_token.cancel();
        } else {
            log::debug!("No abort token for request {request_id}");
        }
        let body = TunnelRequest::Abort(TunnelAbortRequest { request_id });
        self.send_message_to(id, serde_json::to_string(&body).unwrap())
            .await
    }

    /// Send message directly to the user.
    async fn send_message_to(
        &self,
        id: ConnId,
        msg: impl Into<String>,
    ) -> Result<(), WebsocketMessageError> {
        if let Some(session) = self.sessions.get(&id) {
            let message = msg.into();
            log::debug!("Sending message to {id} size={}", message.len());
            // errors if client disconnected abruptly and hasn't been timed-out yet
            Ok(session.send(message)?)
        } else {
            Err(WebsocketMessageError::NoSession(id))
        }
    }

    /// Send message directly to the user.
    async fn broadcast(&self, msg: impl Into<String>) -> Result<(), WebsocketMessageError> {
        log::debug!("Broadcasting message");
        let message = msg.into();

        for session in self.clients.values() {
            // errors if client disconnected abruptly and hasn't been timed-out yet
            session.send(message.clone())?
        }
        Ok(())
    }

    /// Send message directly to the user.
    async fn broadcast_except(
        &self,
        ids: &[ConnId],
        msg: impl Into<String>,
    ) -> Result<(), WebsocketMessageError> {
        log::debug!("Broadcasting message except {ids:?}");
        let message = msg.into();

        for (id, session) in &self.clients {
            if ids.iter().any(|exclude| *exclude == *id) {
                continue;
            }
            // errors if client disconnected abruptly and hasn't been timed-out yet
            session.send(message.clone())?
        }
        Ok(())
    }

    /// Register new session and assign unique ID to this session
    async fn connect(
        &mut self,
        client_id: String,
        sender: bool,
        tx: mpsc::UnboundedSender<Msg>,
    ) -> Result<ConnId, DatabaseError> {
        // register session with random connection ID
        let id = thread_rng().gen::<usize>();

        log::info!("Someone joined {id} sender={sender}");

        self.sessions.insert(id, tx.clone());

        if sender {
            log::info!("Awaiting upsert_connection");
            upsert_connection(&client_id, &id.to_string()).await?;
            log::info!("Awaited upsert_connection");
            CACHE_CONNECTIONS_MAP.write().unwrap().insert(client_id, id);
        } else {
            self.clients.insert(id, tx.clone());
        }

        let count = self.visitor_count.fetch_add(1, Ordering::SeqCst) + 1;
        log::debug!("Visitor count: {count}");

        // send id back
        Ok(id)
    }

    /// Unregister connection from room map and invoke ws api disconnect.
    async fn disconnect(&mut self, conn_id: ConnId) -> Result<(), DatabaseError> {
        log::info!("Someone disconnected {conn_id}");
        let count = self.visitor_count.fetch_sub(1, Ordering::SeqCst) - 1;
        log::debug!("Visitor count: {count}");

        log::info!("Awaiting delete_connection");
        delete_connection(&conn_id.to_string()).await?;
        log::info!("Awaited delete_connection");

        CACHE_CONNECTIONS_MAP
            .write()
            .unwrap()
            .retain(|_, id| *id != conn_id);

        // remove sender
        self.sessions.remove(&conn_id);
        self.clients.remove(&conn_id);

        Ok(())
    }

    pub async fn run(self) -> io::Result<()> {
        let cmd_rx = self.cmd_rx.clone();
        let ctx = Arc::new(RwLock::new(self));
        while let Ok(cmd) = {
            log::info!("Awaiting cmd_rx.recv_async");
            cmd_rx.recv_async().await
        } {
            log::info!("Awaited cmd_rx.recv_async");
            let ctx = ctx.clone();
            tokio::spawn(async move {
                match cmd {
                    Command::Connect {
                        client_id,
                        conn_tx,
                        res_tx,
                        sender,
                    } => {
                        log::info!("Awaiting ctx write Connect");
                        let mut binding = ctx.write().await;
                        log::info!("Awaited ctx write Connect");
                        log::info!("Awaiting connect");
                        let response = binding.connect(client_id, sender, conn_tx).await;
                        log::info!("Awaited connect");
                        drop(binding);
                        match response {
                            Ok(id) => {
                                log::info!("Successfully connected id={id}");
                                if let Err(error) = res_tx.send(id) {
                                    log::error!("Failed to connect {error:?}");
                                }
                            }
                            Err(err) => log::error!("Failed to connect {err:?}"),
                        }
                    }

                    Command::Disconnect { conn } => {
                        log::info!("Awaiting ctx write Disconnect ctx");
                        let mut binding = ctx.write().await;
                        log::info!("Awaited ctx write Disconnect ctx");
                        log::info!("Awaiting disconnect");
                        let response = binding.disconnect(conn).await;
                        log::info!("Awaited disconnect");
                        drop(binding);
                        if let Err(err) = response {
                            log::error!("Failed to disconnect {err:?}");
                        }
                    }

                    Command::RequestStart {
                        request_id,
                        sender,
                        headers_sender,
                        abort_request_token,
                    } => {
                        log::info!("Awaiting ctx write RequestStart ctx");
                        let mut ctx = ctx.write().await;
                        log::info!("Awaited ctx write RequestStart ctx");
                        ctx.senders.insert(request_id, sender);
                        ctx.headers_senders.insert(request_id, headers_sender);
                        ctx.abort_request_tokens
                            .insert(request_id, abort_request_token);
                        drop(ctx);
                    }

                    Command::RequestEnd { request_id } => {
                        log::info!("Awaiting ctx write RequestEnd ctx");
                        let mut ctx = ctx.write().await;
                        log::info!("Awaited ctx write RequestEnd ctx");
                        ctx.senders.remove(&request_id);
                        ctx.headers_senders.remove(&request_id);
                        ctx.abort_request_tokens.remove(&request_id);
                        drop(ctx);
                    }

                    Command::Response { response, conn_id } => {
                        let request_id = response.request_id;

                        if let (Some(status), Some(headers)) = (response.status, &response.headers)
                        {
                            log::info!("Awaiting ctx write Response ctx");
                            let mut ctx = ctx.write().await;
                            log::info!("Awaited ctx write Response ctx");
                            let headers_senders = ctx.headers_senders.remove(&request_id);
                            if let Some(sender) = headers_senders {
                                if sender
                                    .send(RequestHeaders {
                                        status,
                                        headers: headers.clone(),
                                    })
                                    .is_err()
                                {
                                    log::warn!("Header sender dropped for request {}", request_id);
                                    ctx.headers_senders.remove(&request_id);
                                    log::info!("Awaiting abort_request");
                                    let resp = ctx.abort_request(conn_id, request_id).await;
                                    log::info!("Awaited abort_request");
                                    if let Err(err) = resp {
                                        log::error!("Failed to abort request {request_id} {err:?}");
                                    }
                                }
                            } else {
                                log::error!(
                                    "unexpected binary message {} (size {})",
                                    request_id,
                                    response.bytes.len()
                                );
                            }
                            drop(ctx);
                        }

                        {
                            log::info!("Awaiting ctx read senders");
                            let binding = ctx.read().await;
                            log::info!("Awaited ctx read senders");
                            let senderrr = binding.senders.get(&request_id);
                            if let Some(sender) = senderrr {
                                if sender.send(response).is_err() {
                                    log::debug!("Sender dropped for request {}", request_id);
                                    log::info!("Awaiting ctx write WsRequest ctx write");
                                    let mut binding = ctx.write().await;
                                    log::info!("Awaited ctx write WsRequest ctx write");
                                    binding.senders.remove(&request_id);
                                    drop(binding);
                                    log::info!("Awaiting abort_request");
                                    if let Err(err) =
                                        ctx.read().await.abort_request(conn_id, request_id).await
                                    {
                                        log::error!("Failed to abort request {request_id} {err:?}");
                                    }
                                    log::info!("Awaited abort_request");
                                }
                            } else {
                                log::error!(
                                    "unexpected binary message {} (size {})",
                                    request_id,
                                    response.bytes.len()
                                );
                            }
                        }
                    }

                    Command::WsRequest {
                        conn_id,
                        client_id,
                        request_id,
                        body,
                    } => {
                        log::info!("Awaiting get_connection_id");
                        let resp = get_connection_id(&client_id).await;
                        log::info!("Awaited get_connection_id");
                        match resp {
                            Ok(client_conn_id) => {
                                let value: Value = serde_json::from_str(&body).unwrap();
                                let body = TunnelRequest::Ws(TunnelWsRequest {
                                    conn_id,
                                    request_id,
                                    body: value,
                                    connection_id: None,
                                });
                                log::info!("Awaiting WsRequest ctx");
                                let binding = ctx.read().await;
                                log::info!("Awaited WsRequest ctx");
                                log::info!("Awaiting send_message_to ctx");
                                let response = binding
                                    .send_message_to(
                                        client_conn_id,
                                        serde_json::to_string(&body).unwrap(),
                                    )
                                    .await;
                                log::info!("Awaited send_message_to ctx");
                                drop(binding);

                                if let Err(error) = response {
                                    log::error!(
                                        "Failed to send WsRequest to {client_conn_id}: {error:?}"
                                    );
                                }
                                log::info!("Awaiting ctx write WsRequest ctx write");
                                let mut binding = ctx.write().await;
                                log::info!("Awaited ctx write WsRequest ctx write");
                                binding.ws_requests.insert(request_id, conn_id);
                                drop(binding);
                            }
                            Err(err) => {
                                log::error!("Failed to get connection id: {err:?}");
                            }
                        }
                    }

                    Command::WsMessage { message } => {
                        if let Some(to_connection_ids) = message.to_connection_ids {
                            for conn_id in to_connection_ids {
                                log::info!("Awaiting WsMessage ctx");
                                let binding = ctx.read().await;
                                log::info!("Awaited WsMessage ctx");
                                log::info!("Awaiting send_message_to ctx");
                                let response = binding
                                    .send_message_to(conn_id, message.body.to_string())
                                    .await;
                                log::info!("Awaited send_message_to ctx");
                                drop(binding);
                                if let Err(error) = response {
                                    log::error!(
                                        "Failed to send WsResponse to {conn_id}: {error:?}"
                                    );
                                }
                            }
                        } else if let Some(exclude_connection_ids) = message.exclude_connection_ids
                        {
                            log::info!("Awaiting WsMessage2 ctx");
                            let binding = ctx.read().await;
                            log::info!("Awaited WsMessage2 ctx");
                            log::info!("Awaiting broadcast_except ctx");
                            let response = binding
                                .broadcast_except(&exclude_connection_ids, message.body.to_string())
                                .await;
                            log::info!("Awaited broadcast_except ctx");
                            drop(binding);
                            if let Err(error) = response {
                                log::error!("Failed to broadcast_except WsMessage: {error:?}");
                            }
                        } else {
                            log::info!("Awaiting WsMessage3 ctx");
                            let binding = ctx.read().await;
                            log::info!("Awaited WsMessage3 ctx");
                            log::info!("Awaiting broadcast ctx");
                            let response = binding.broadcast(message.body.to_string()).await;
                            log::info!("Awaited broadcast ctx");
                            drop(binding);
                            if let Err(error) = response {
                                log::error!("Failed to broadcast WsMessage: {error:?}");
                            }
                        }
                    }

                    Command::WsResponse { response } => {
                        log::info!("Awaiting WsResponse1 ctx");
                        let binding = ctx.read().await;
                        log::info!("Awaited WsResponse1 ctx");
                        let ws_id = binding.ws_requests.get(&response.request_id).cloned();
                        drop(binding);
                        if let Some(ws_id) = ws_id {
                            log::info!("Awaiting WsResponse2 ctx");
                            let binding = ctx.read().await;
                            log::info!("Awaited WsResponse2 ctx");
                            log::info!("Awaiting send_message_to ctx");
                            let response = binding
                                .send_message_to(ws_id, response.body.to_string())
                                .await;
                            log::info!("Awaited send_message_to ctx");
                            drop(binding);
                            if let Err(error) = response {
                                log::error!("Failed to send WsResponse to {ws_id}: {error:?}");
                            }
                        } else {
                            log::error!("unexpected ws response {}", response.request_id,);
                        }
                    }

                    Command::Message { conn, msg } => {
                        log::info!("Awaiting Message ctx");
                        let binding = ctx.read().await;
                        log::info!("Awaited Message ctx");
                        log::info!("Awaiting send_message_to ctx");
                        let response = binding.send_message_to(conn, &msg).await;
                        log::info!("Awaited send_message_to ctx");
                        drop(binding);
                        if let Err(error) = response {
                            log::error!("Failed to send message to {conn}: {msg:?}: {error:?}");
                        }
                    }
                }
            });
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum WsRequestError {
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

#[derive(Error, Debug)]
pub enum ConnectionIdError {
    #[error("Invalid Connection ID '{0}'")]
    Invalid(String),
    #[error("Connection ID not found for client_id '{0}'")]
    NotFound(String),
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

static CACHE_CONNECTIONS_MAP: once_cell::sync::Lazy<std::sync::RwLock<HashMap<String, usize>>> =
    once_cell::sync::Lazy::new(|| std::sync::RwLock::new(HashMap::new()));

/// Handle and command sender for chat server.
///
/// Reduces boilerplate of setting up response channels in WebSocket handlers.
#[derive(Debug, Clone)]
pub struct ChatServerHandle {
    cmd_tx: flume::Sender<Command>,
}

impl ChatServerHandle {
    /// Register client message sender and obtain connection ID.
    pub async fn connect(
        &self,
        client_id: &str,
        sender: bool,
        conn_tx: mpsc::UnboundedSender<String>,
    ) -> ConnId {
        let (res_tx, res_rx) = oneshot::channel();

        // unwrap: chat server should not have been dropped
        self.cmd_tx
            .send_async(Command::Connect {
                conn_tx,
                res_tx,
                client_id: client_id.to_string(),
                sender,
            })
            .await
            .unwrap();

        // unwrap: chat server does not drop out response channel
        res_rx.await.unwrap()
    }

    /// Broadcast message to current room.
    pub async fn send_message(&self, conn: ConnId, msg: impl Into<String>) {
        // unwrap: chat server should not have been dropped
        self.cmd_tx
            .send_async(Command::Message {
                msg: msg.into(),
                conn,
            })
            .await
            .unwrap();
    }

    pub async fn ws_request(
        &self,
        conn_id: usize,
        client_id: &str,
        msg: impl Into<String>,
    ) -> Result<(), WsRequestError> {
        let request_id = thread_rng().gen::<usize>();

        self.cmd_tx
            .send_async(Command::WsRequest {
                request_id,
                conn_id,
                client_id: client_id.to_string(),
                body: msg.into(),
            })
            .await
            .unwrap();
        Ok(())
    }

    pub async fn ws_message(&self, message: TunnelWsResponse) -> Result<(), WsRequestError> {
        self.cmd_tx
            .send_async(Command::WsMessage { message })
            .await
            .unwrap();

        Ok(())
    }

    pub async fn ws_response(&self, response: TunnelWsResponse) -> Result<(), WsRequestError> {
        self.cmd_tx
            .send_async(Command::WsResponse { response })
            .await
            .unwrap();

        Ok(())
    }

    /// Unregister message sender and broadcast disconnection message to current room.
    pub async fn disconnect(&self, conn: ConnId) {
        // unwrap: chat server should not have been dropped
        self.cmd_tx
            .send_async(Command::Disconnect { conn })
            .await
            .unwrap();
    }

    pub async fn request_start(
        &self,
        request_id: usize,
        sender: UnboundedSender<TunnelResponse>,
        headers_sender: oneshot::Sender<RequestHeaders>,
        abort_request_token: CancellationToken,
    ) {
        self.cmd_tx
            .send_async(Command::RequestStart {
                request_id,
                sender,
                headers_sender,
                abort_request_token,
            })
            .await
            .unwrap();
    }

    pub async fn request_end(&self, request_id: usize) {
        self.cmd_tx
            .send_async(Command::RequestEnd { request_id })
            .await
            .unwrap();
    }

    pub async fn response(&self, conn_id: ConnId, response: TunnelResponse) {
        self.cmd_tx
            .send_async(Command::Response { conn_id, response })
            .await
            .unwrap();
    }

    pub async fn get_connection_id(&self, client_id: &str) -> Result<usize, ConnectionIdError> {
        crate::ws::server::get_connection_id(client_id).await
    }
}

pub async fn get_connection_id(client_id: &str) -> Result<usize, ConnectionIdError> {
    let existing = {
        let lock = CACHE_CONNECTIONS_MAP.read().unwrap();
        lock.get(client_id).copied()
    };
    if let Some(conn_id) = existing {
        Ok(conn_id)
    } else {
        let tunnel_ws_id = select_connection(client_id)
            .await?
            .ok_or(ConnectionIdError::NotFound(client_id.to_string()))?
            .tunnel_ws_id;

        let conn_id = tunnel_ws_id
            .parse::<usize>()
            .map_err(|_| ConnectionIdError::Invalid(tunnel_ws_id))?;

        CACHE_CONNECTIONS_MAP
            .write()
            .unwrap()
            .insert(client_id.to_string(), conn_id);

        Ok(conn_id)
    }
}
