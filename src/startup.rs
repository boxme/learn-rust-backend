use std::{io::Error, net::TcpListener};

use actix_web::{dev::Server, web, App, HttpServer};

use crate::routes;

pub fn run(tcp_listener: TcpListener) -> Result<Server, Error> {
    let server = HttpServer::new(|| {
        App::new()
            .route("/health_check", web::get().to(routes::health_check))
            .route("/subscriptions", web::post().to(routes::subscribe))
    })
    .listen(tcp_listener)?
    .run();

    Ok(server)
}
