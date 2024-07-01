#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{collections::HashMap, fmt::Display, task::Poll, time::SystemTime};

use bytes::Bytes;
use futures_util::{Future, Stream};
use log::debug;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::EnumString;
use thiserror::Error;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_util::sync::CancellationToken;

#[cfg(feature = "base64")]
static BASE64_TUNNEL_RESPONSE_PREFIX: &str = "TUNNEL_RESPONSE:";

#[derive(Debug, Serialize, Deserialize, EnumString, PartialEq, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TunnelEncoding {
    Binary,
    #[cfg(feature = "base64")]
    Base64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TunnelWsResponse {
    pub request_id: usize,
    pub body: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_connection_ids: Option<Vec<usize>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_connection_ids: Option<Vec<usize>>,
}

#[derive(Debug)]
pub struct TunnelResponse {
    pub request_id: usize,
    pub packet_id: u32,
    pub last: bool,
    pub bytes: Bytes,
    pub status: Option<u16>,
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, EnumString)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum Method {
    Head,
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Options,
}

impl Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Method::Head => "HEAD",
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
            Method::Options => "OPTIONS",
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum TunnelRequest {
    Http(TunnelHttpRequest),
    Ws(TunnelWsRequest),
    Abort(TunnelAbortRequest),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TunnelHttpRequest {
    pub request_id: usize,
    pub method: Method,
    pub path: String,
    pub query: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<Value>,
    pub encoding: TunnelEncoding,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TunnelWsRequest {
    pub conn_id: usize,
    pub request_id: usize,
    pub body: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TunnelAbortRequest {
    pub request_id: usize,
}

impl From<Bytes> for TunnelResponse {
    fn from(bytes: Bytes) -> Self {
        let mut data = bytes.slice(13..);
        let request_id = usize::from_be_bytes(bytes[..8].try_into().unwrap());
        let packet_id = u32::from_be_bytes(bytes[8..12].try_into().unwrap());
        let last = u8::from_be_bytes(bytes[12..13].try_into().unwrap()) == 1;
        let (status, headers) = if packet_id == 1 {
            let status = u16::from_be_bytes(data[..2].try_into().unwrap());
            data = data.slice(2..);
            let len = u32::from_be_bytes(data[..4].try_into().unwrap()) as usize;
            let headers_bytes = &data.slice(4..(4 + len));
            data = data.slice((4 + len)..);
            (
                Some(status),
                Some(serde_json::from_slice(headers_bytes).unwrap()),
            )
        } else {
            (None, None)
        };

        TunnelResponse {
            request_id,
            packet_id,
            last,
            bytes: data,
            status,
            headers,
        }
    }
}

#[cfg(feature = "base64")]
#[derive(Debug, Error)]
pub enum Base64DecodeError {
    #[error("Invalid Content: {0:?}")]
    InvalidContent(String),
    #[error(transparent)]
    Decode(#[from] base64::DecodeError),
}

#[cfg(feature = "base64")]
impl TryFrom<&str> for TunnelResponse {
    type Error = Base64DecodeError;

    fn try_from(base64: &str) -> Result<Self, Self::Error> {
        use base64::{engine::general_purpose, Engine};

        let base64 = base64.strip_prefix(BASE64_TUNNEL_RESPONSE_PREFIX).ok_or(
            Base64DecodeError::InvalidContent("Invalid TunnelRequest base64 data string".into()),
        )?;

        let request_id_pos =
            base64
                .chars()
                .position(|c| c == '|')
                .ok_or(Base64DecodeError::InvalidContent(
                    "Missing request_id. Expected '|' delimiter".into(),
                ))?;
        let request_id = base64[..request_id_pos].parse::<usize>().unwrap();

        let packet_id_pos = base64
            .chars()
            .skip(request_id_pos + 2)
            .position(|c| c == '|')
            .ok_or(Base64DecodeError::InvalidContent(
                "Missing packet_id. Expected '|' delimiter".into(),
            ))?;
        let packet_id = base64[request_id_pos + 1..packet_id_pos]
            .parse::<u32>()
            .unwrap();

        let last_pos = packet_id_pos + 2; // 1 (delimiter) + 1 (u8 bool byte)
        let last = base64[packet_id_pos + 1..last_pos].parse::<u8>().unwrap() == 1;

        let (status, headers) = if packet_id == 1 {
            let status_pos = last_pos + 3; // 3 digit status code
            let status = base64[last_pos..status_pos].parse::<u16>().unwrap();

            let headers_pos = base64
                .chars()
                .skip(status_pos + 2)
                .position(|c| c == '}')
                .ok_or(Base64DecodeError::InvalidContent(
                    "Missing headers. Expected '}' delimiter".into(),
                ))?;

            let headers_str = &base64[status_pos + 1..headers_pos];

            (
                Some(status),
                Some(serde_json::from_str(headers_str).unwrap()),
            )
        } else {
            (None, None)
        };

        let bytes = Bytes::from(general_purpose::STANDARD.decode(base64)?);

        Ok(TunnelResponse {
            request_id,
            packet_id,
            last,
            bytes,
            status,
            headers,
        })
    }
}

#[cfg(feature = "base64")]
impl TryFrom<String> for TunnelResponse {
    type Error = Base64DecodeError;

    fn try_from(base64: String) -> Result<Self, Self::Error> {
        base64.as_str().try_into()
    }
}

#[derive(Debug, Error)]
pub enum TunnelStreamError {
    #[error("TunnelStream aborted")]
    Aborted,
}

pub struct TunnelStream<'a, F: Future<Output = ()>> {
    start: SystemTime,
    request_id: usize,
    time_to_first_byte: Option<SystemTime>,
    packet_count: u32,
    byte_count: usize,
    done: bool,
    rx: UnboundedReceiver<TunnelResponse>,
    on_end: &'a dyn Fn(usize) -> F,
    packet_queue: Vec<TunnelResponse>,
    abort_token: CancellationToken,
}

impl<'a, F: Future<Output = ()>> TunnelStream<'a, F> {
    pub fn new(
        request_id: usize,
        rx: UnboundedReceiver<TunnelResponse>,
        abort_token: CancellationToken,
        on_end: &'a impl Fn(usize) -> F,
    ) -> TunnelStream<'a, F> {
        TunnelStream {
            start: SystemTime::now(),
            request_id,
            time_to_first_byte: None,
            packet_count: 0,
            byte_count: 0,
            done: false,
            rx,
            on_end,
            packet_queue: vec![],
            abort_token,
        }
    }
}

fn return_polled_bytes<F: Future<Output = ()>>(
    stream: &mut TunnelStream<F>,
    response: TunnelResponse,
) -> std::task::Poll<Option<Result<Bytes, TunnelStreamError>>> {
    if stream.time_to_first_byte.is_none() {
        stream.time_to_first_byte = Some(SystemTime::now());
    }

    stream.packet_count += 1;

    debug!(
        "Received packet for {} {} {} bytes last={}",
        stream.request_id,
        stream.packet_count,
        response.bytes.len(),
        response.last,
    );

    if response.last {
        stream.done = true;
    }

    stream.byte_count += response.bytes.len();

    Poll::Ready(Some(Ok(response.bytes)))
}

impl<F: Future<Output = ()>> Stream for TunnelStream<'_, F> {
    type Item = Result<Bytes, TunnelStreamError>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let stream = self.get_mut();
        if stream.abort_token.is_cancelled() {
            return Poll::Ready(Some(Err(TunnelStreamError::Aborted)));
        }
        if stream.done {
            let end = SystemTime::now();

            debug!(
                "Byte count: {} (received {} packet{}, took {}ms total, {}ms to first byte)",
                stream.byte_count,
                stream.packet_count,
                if stream.packet_count == 1 { "" } else { "s" },
                end.duration_since(stream.start).unwrap().as_millis(),
                stream
                    .time_to_first_byte
                    .map(|t| t.duration_since(stream.start).unwrap().as_millis())
                    .map(|t| t.to_string())
                    .unwrap_or("N/A".into())
            );

            (stream.on_end)(stream.request_id);

            return Poll::Ready(None);
        }

        debug!("Waiting for next packet");
        let response = match stream.rx.poll_recv(cx) {
            Poll::Ready(Some(response)) => response,
            Poll::Pending => {
                debug!("Pending...");
                return Poll::Pending;
            }
            Poll::Ready(None) => {
                debug!("Finished");
                return Poll::Ready(None);
            }
        };

        if response.packet_id == 1 && response.last {
            return return_polled_bytes(stream, response);
        }

        if stream
            .packet_queue
            .first()
            .map(|n| n.packet_id == stream.packet_count + 1)
            .is_some_and(|n| n)
        {
            let response = stream.packet_queue.remove(0);
            debug!("Sending queued packet {}", response.packet_id);
            return return_polled_bytes(stream, response);
        }

        if response.packet_id > stream.packet_count + 1 {
            debug!(
                "Received future packet {}. Waiting for packet {} before continuing",
                response.packet_id,
                stream.packet_count + 1
            );
            if let Some(pos) = stream
                .packet_queue
                .iter()
                .position(|r| r.packet_id > response.packet_id)
            {
                stream.packet_queue.insert(pos, response);
            } else {
                stream.packet_queue.push(response);
            }
            return Poll::Pending;
        }

        return_polled_bytes(stream, response)
    }
}
