use actix_htmx::Htmx;
use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    route, web, HttpResponse, Responder, Scope,
};
use maud::{html, DOCTYPE};

mod info;
#[cfg(feature = "qobuz")]
mod qobuz;
mod scan;
#[cfg(feature = "tidal")]
mod tidal;
pub(crate) mod util;

pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    info::bind_services(scan::bind_services(tidal::bind_services(
        qobuz::bind_services(scope.service(index_endpoint)),
    )))
}

#[route("", method = "GET")]
pub async fn index_endpoint(
    _htmx: Htmx,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<impl Responder, actix_web::Error> {
    let mut response = HttpResponse::Ok();
    response.content_type("text/html");

    Ok(response.body(
        html! {
            (DOCTYPE)
            html {
                head {
                    title { "MoosicBox Admin" }
                    script
                        src="https://unpkg.com/htmx.org@2.0.2"
                        integrity="sha384-Y7hw+L/jvKeWIRRkqWYfPcvVxHzVzn5REgzbawhxAuQGwX1XWe70vji+VSeHOThJ"
                        crossorigin="anonymous"
                        {}
                }
                body {
                    h1 { "MoosicBox Admin" }
                    hr {}
                    h2 { "Server Info" }
                    (info::info(&**data.database).await?)
                    hr {}
                    h2 { "Scan" }
                    (scan::scan(&**data.database).await?)
                    (if cfg!(feature = "tidal") { html! {
                        hr {}
                        h2 { "Tidal" }
                        (tidal::settings(&**data.database).await.map_err(ErrorInternalServerError)?)
                    } } else { html!{} })
                    (if cfg!(feature = "qobuz") { html! {
                        hr {}
                        h2 { "Qobuz" }
                        (qobuz::settings(&**data.database).await.map_err(ErrorInternalServerError)?)
                    } } else { html!{} })
                }
            }
        }
        .into_string(),
    ))
}
