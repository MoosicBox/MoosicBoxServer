use actix_web::{
    web::{self},
    Error, HttpRequest, HttpResponse, Result,
};
use actix_web_actors::ws;
use lambda_web::actix_web::{self, App, HttpServer};
use lambda_web::{is_running_on_lambda, run_actix_on_lambda};

mod server;
use self::server::MyWebSocket;

async fn websocket(req: HttpRequest, stream: web::Payload) -> Result<HttpResponse, Error> {
    ws::start(MyWebSocket::new(), &req, stream)
}

#[actix_web::main]
async fn main() -> Result<(), lambda_runtime::Error> {
    let service_port = 8000;

    let factory = move || {
        /*let cors = Cors::default()
        .allow_any_origin()
        .allowed_methods(vec!["GET", "POST"])
        .allowed_headers(vec![http::header::AUTHORIZATION, http::header::ACCEPT])
        .allowed_header(http::header::CONTENT_TYPE)
        .supports_credentials()
        .max_age(3600);*/

        App::new()
            //.wrap(cors)
            //.wrap(middleware::Compress::default())
            .service(web::resource("/").route(web::get().to(websocket)))
    };
    if is_running_on_lambda() {
        run_actix_on_lambda(factory).await?;
    } else {
        HttpServer::new(factory)
            .bind(format!("0.0.0.0:{service_port}"))?
            .run()
            .await?;
    }
    Ok(())
}
