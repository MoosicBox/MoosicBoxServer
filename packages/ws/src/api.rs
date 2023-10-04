use aws_lambda_events::apigw::ApiGatewayProxyRequest;
use lambda_runtime::LambdaEvent;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WebsocketConnectError {
    #[error("test")]
    Unknown,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    message: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub status_code: u16,
    pub body: String,
}

pub async fn connect(
    event: LambdaEvent<ApiGatewayProxyRequest>,
) -> Result<Response, WebsocketConnectError> {
    println!("in ws! {:?}", event.payload.http_method);
    Ok(Response {
        status_code: 200,
        body: "Connected".into(),
    })
}

pub async fn message(event: LambdaEvent<Request>) -> Result<Response, WebsocketConnectError> {
    println!(
        "in message! {}",
        event.payload.message.unwrap_or("(none)".into())
    );
    Ok(Response {
        status_code: 200,
        body: "Connected".into(),
    })
}
