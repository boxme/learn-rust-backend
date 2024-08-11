use std::{
    io::{stdout, Error},
    net::TcpListener,
};

use sqlx::postgres::PgPoolOptions;
use zero2prod::{
    configuration::get_configuration,
    startup::run,
    telemetry::{get_subscriber, init_subscriber},
};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let subscriber = get_subscriber("zero2pod".into(), "info".into(), stdout);
    init_subscriber(subscriber);

    // Panic if we can't read configuration
    let configuration = get_configuration().expect("Failed to read configs");
    let connection_pool = PgPoolOptions::new().connect_lazy_with(configuration.database.with_db());

    let address = format!(
        "{}:{}",
        configuration.application.host, configuration.application.port
    );
    let listener = TcpListener::bind(address)?;

    run(listener, connection_pool)?.await
}
