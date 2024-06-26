use crate::{ws::handler, CHAT_SERVER_HANDLE};
use actix_web::{
    get,
    web::{self, Json},
    Result,
};
use actix_web::{route, HttpResponse};
use log::info;
use serde_json::{json, Value};
use tokio::task::spawn_local;

#[route("/health", method = "GET")]
pub async fn health_endpoint() -> Result<Json<Value>> {
    info!("Healthy");
    Ok(Json(json!({"healthy": true})))
}

#[allow(clippy::future_not_send)]
#[get("/ws")]
pub async fn websocket(
    req: actix_web::HttpRequest,
    stream: web::Payload,
) -> Result<HttpResponse, actix_web::Error> {
    let (response, session, msg_stream) = actix_ws::handle(&req, stream)?;

    // spawn websocket handler (and don't await it) so that the response is returned immediately
    spawn_local(handler::chat_ws(
        CHAT_SERVER_HANDLE
            .read()
            .await
            .as_ref()
            .expect("No ChatServerHandle available")
            .clone(),
        session,
        msg_stream,
    ));

    Ok(response)
}
