use crate::auth::SignatureAuthorized;
use crate::ws::handler;
use crate::CHAT_SERVER_HANDLE;
use actix_web::HttpResponse;
use actix_web::{
    get,
    web::{self},
    Result,
};
use serde::Deserialize;
use tokio::task::spawn_local;

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    client_id: String,
    sender: Option<bool>,
}

#[get("/ws")]
pub async fn websocket(
    req: actix_web::HttpRequest,
    stream: web::Payload,
    query: web::Query<ConnectRequest>,
    _: SignatureAuthorized,
) -> Result<HttpResponse, actix_web::Error> {
    let chat_server = CHAT_SERVER_HANDLE.read().await.as_ref().unwrap().clone();
    let (res, session, msg_stream) = actix_ws::handle(&req, stream)?;

    // spawn websocket handler (and don't await it) so that the response is returned immediately
    spawn_local(handler::chat_ws(
        chat_server,
        session,
        msg_stream,
        query.client_id.clone(),
        query.sender.unwrap_or(false),
    ));

    Ok(res)
}
