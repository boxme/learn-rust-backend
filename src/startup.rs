use std::{io::Error, net::TcpListener};

use actix_web::{dev::Server, web, App, HttpServer};
use sqlx::PgPool;

use crate::routes;

pub fn run(tcp_listener: TcpListener, db_pool: PgPool) -> Result<Server, Error> {
    // Wrap db_pool in a smart pointer, which boils down to an Arc smart pointer
    let db_pool = web::Data::new(db_pool);

    // Capture `connection` from the surrounding environment
    let server = HttpServer::new(move || {
        App::new()
            .route("/health_check", web::get().to(routes::health_check))
            .route("/subscriptions", web::post().to(routes::subscribe))
            // Get a pointer copy and attach it to the application state
            .app_data(db_pool.clone())
    })
    .listen(tcp_listener)?
    .run();

    Ok(server)
}
