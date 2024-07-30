use actix_web::{dev::Server, web, App, HttpResponse, HttpServer};
use std::{io::Error, net::TcpListener};

async fn health_check() -> HttpResponse {
    HttpResponse::Ok().finish()
}

pub fn run(tcp_listener: TcpListener) -> Result<Server, Error> {
    let server = HttpServer::new(|| App::new().route("/health_check", web::get().to(health_check)))
        .listen(tcp_listener)?
        .run();

    Ok(server)
}
