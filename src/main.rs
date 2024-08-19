use std::io::{stdout, Error};

use zero2prod::{
    configuration::get_configuration,
    startup::Application,
    telemetry::{get_subscriber, init_subscriber},
};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let subscriber = get_subscriber("zero2pod".into(), "info".into(), stdout);
    init_subscriber(subscriber);

    // Panic if we can't read configuration
    let configuration = get_configuration().expect("Failed to read configs");
    let server = Application::build(configuration).await?;
    server.run_until_stopped().await?;
    Ok(())
}
