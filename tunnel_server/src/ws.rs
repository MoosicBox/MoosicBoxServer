pub mod api;
pub mod db;
pub mod handler;
pub mod server;

/// Connection ID.
pub type ConnId = usize;

/// Message sent to a room/client.
pub type Msg = String;
