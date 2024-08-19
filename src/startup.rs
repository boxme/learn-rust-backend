use std::{io::Error, net::TcpListener};

use actix_web::{dev::Server, web, App, HttpServer};
use sqlx::{postgres::PgPoolOptions, PgPool};
use tracing_actix_web::TracingLogger;

use crate::{
    configuration::{DatabaseSettings, Settings},
    email_client::EmailClient,
    routes,
};

// New type to hold newly built server and its port
pub struct Application {
    port: u16,
    server: Server,
}

impl Application {
    pub async fn build(configuration: Settings) -> Result<Self, std::io::Error> {
        let connection_pool = get_connection_pool(&configuration.database);

        // build an Emailclient using configuration
        let sender_email = configuration
            .email_client
            .sender()
            .expect("Invalid sender email address");
        let timeout = configuration.email_client.timeout();
        let email_client = EmailClient::new(
            configuration.email_client.base_url,
            sender_email,
            configuration.email_client.authorization_token,
            timeout,
        );

        let address = format!(
            "{}:{}",
            configuration.application.host, configuration.application.port
        );
        let listener = TcpListener::bind(address)?;
        let port = listener.local_addr().unwrap().port();
        let server = run(listener, connection_pool, email_client)?;

        // Keep the bound port in one of `Application`'s fields
        Ok(Self { port, server })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    // Function returns only when the application has stopped.
    pub async fn run_until_stopped(self) -> Result<(), std::io::Error> {
        self.server.await
    }
}

pub fn get_connection_pool(configuration: &DatabaseSettings) -> PgPool {
    PgPoolOptions::new().connect_lazy_with(configuration.with_db())
}

pub fn run(
    tcp_listener: TcpListener,
    db_pool: PgPool,
    email_client: EmailClient,
) -> Result<Server, Error> {
    // Wrap db_pool in a smart pointer, which boils down to an Arc smart pointer
    let db_pool = web::Data::new(db_pool);

    let email_client = web::Data::new(email_client);

    // Capture `connection` from the surrounding environment
    let server = HttpServer::new(move || {
        App::new()
            // Middlewares are added using wrap method
            .wrap(TracingLogger::default())
            .route("/health_check", web::get().to(routes::health_check))
            .route("/subscriptions", web::post().to(routes::subscribe))
            // Get a pointer copy and attach it to the application state
            .app_data(db_pool.clone())
            .app_data(email_client.clone())
    })
    .listen(tcp_listener)?
    .run();

    Ok(server)
}
