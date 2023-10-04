use std::time::{Duration, Instant};

use actix::prelude::*;
use actix_web_actors::ws;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

pub struct MyWebSocket {
    hb: Instant,
}

impl MyWebSocket {
    pub fn new() -> Self {
        Self { hb: Instant::now() }
    }

    // This function will run on an interval, every 5 seconds to check
    // that the connection is still alive. If it's been more than
    // 10 seconds since the last ping, we'll close the connection.
    fn hb(&self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                ctx.stop();
                return;
            }

            ctx.ping(b"");
        });
    }
}

impl Actor for MyWebSocket {
    type Context = ws::WebsocketContext<Self>;

    // Start the heartbeat process for this connection
    fn started(&mut self, ctx: &mut Self::Context) {
        self.hb(ctx);
    }
}

// The `StreamHandler` trait is used to handle the messages that are sent over the socket.
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for MyWebSocket {
    // The `handle()` function is where we'll determine the response
    // to the client's messages. So, for example, if we ping the client,
    // it should respond with a pong. These two messages are necessary
    // for the `hb()` function to maintain the connection status.
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            // Ping/Pong will be used to make sure the connection is still alive
            Ok(ws::Message::Ping(msg)) => {
                self.hb = Instant::now();
                println!("Received {msg:?}");
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.hb = Instant::now();
            }
            // Text will echo any text received back to the client (for now)
            Ok(ws::Message::Text(text)) => {
                println!("Received text {text:?}");
                ctx.text(text)
            }
            // Close will close the socket
            Ok(ws::Message::Close(reason)) => {
                ctx.close(reason);
                ctx.stop();
            }
            _ => ctx.stop(),
        }
    }
}
