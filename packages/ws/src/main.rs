use actix_web::{error::ErrorInternalServerError, Result};
use lambda_runtime::service_fn;

#[actix_rt::main]
async fn main() -> Result<(), actix_web::Error> {
    lambda_runtime::run(service_fn(moosicbox_ws::api::connect))
        .await
        .map_err(|e| ErrorInternalServerError(format!("Error: {e:?}")))?;
    Ok(())
}
