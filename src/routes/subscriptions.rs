use actix_web::{web, HttpResponse, ResponseError};
use anyhow::Context;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use reqwest::StatusCode;
use sqlx::{types::chrono::Utc, Executor, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    domain::{NewSubscriber, SubscriberEmail, SubscriberName},
    email_client::EmailClient,
    startup::ApplicationBaseUrl,
};

#[derive(serde::Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}

impl TryFrom<FormData> for NewSubscriber {
    type Error = String;

    fn try_from(form: FormData) -> Result<Self, Self::Error> {
        let name = SubscriberName::parse(form.name)?;
        let email = SubscriberEmail::parse(form.email)?;
        Ok(Self { email, name })
    }
}

#[derive(thiserror::Error)]
pub enum SubscriberError {
    #[error("{0}")]
    ValidationError(String),
    // Transparent delegates both `Display`'s and `source`'s implementation
    // to the type wrapped by `UnexpectedError`
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

// Write custom implementation of Debug
impl std::fmt::Debug for SubscriberError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for SubscriberError {
    fn status_code(&self) -> reqwest::StatusCode {
        match self {
            SubscriberError::ValidationError(_) => StatusCode::BAD_REQUEST,
            SubscriberError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<String> for SubscriberError {
    fn from(value: String) -> Self {
        Self::ValidationError(value)
    }
}

// New error type that wraps sqlx:Error
pub struct StoreTokenError(sqlx::Error);

impl std::fmt::Display for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "A database error was encountered while trying to store a subscription token."
        )
    }
}

impl std::error::Error for StoreTokenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // Compiler transparently casts '&sqlx:Error' into '&dyn Error'
        Some(&self.0)
    }
}

impl std::fmt::Debug for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

#[tracing::instrument(
    name = "Adding a new subscriber",
    skip(form, pool, email_client, base_url),
    fields(
        subscriber_email = %form.email,
        subscriber_name = %form.name
    )
)]
pub async fn subscribe(
    form: web::Form<FormData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    base_url: web::Data<ApplicationBaseUrl>,
) -> Result<HttpResponse, SubscriberError> {
    // `web::Form` is a wrapper around `FormData`
    // `form.0` gives us access to the underlying `FormData`
    let new_subscriber = form.0.try_into()?;
    let mut transaction = pool
        .begin()
        .await
        .context("Failed to acquire a Postgres connection from the pool")?;

    let subscriber_id = insert_subscriber(&mut transaction, &new_subscriber)
        .await
        .context("Failed to insert new subscriber in the database.")?;

    let subscription_token = generate_subscription_token();
    store_token(&mut transaction, subscriber_id, &subscription_token)
        .await
        .context("Failed to store the confirmation token for a new subscriber.")?;

    transaction
        .commit()
        .await
        .context("Failed to commit SQL transaction to store a new subscriber.")?;

    send_confirmation_email(
        &email_client,
        new_subscriber,
        &base_url.0,
        &subscription_token,
    )
    .await
    .context("Failed to send a confirmation email.")?;

    Ok(HttpResponse::Ok().finish())
}

#[tracing::instrument(
    name = "Saving new subscriber details in the database",
    skip(new_subscriber, transaction)
)]
pub async fn insert_subscriber(
    transaction: &mut Transaction<'_, Postgres>,
    new_subscriber: &NewSubscriber,
) -> Result<Uuid, sqlx::Error> {
    let subscriber_id = Uuid::new_v4();
    let query = sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subsribed_at, status)
        VALUES ($1, $2, $3, $4, 'pending_confirmation')
        "#,
        subscriber_id,
        new_subscriber.email.as_ref(),
        new_subscriber.name.as_ref(),
        Utc::now()
    );

    transaction.execute(query).await.map_err(|e| {
        e
        // Using the `?` operator to return early
        // if the function failed, returning a sqlx::Error
    })?;

    Ok(subscriber_id)
}

#[tracing::instrument(
    name = "Send a confirmation email to a new subscriber",
    skip(email_client, new_subscriber)
)]
pub async fn send_confirmation_email(
    email_client: &EmailClient,
    new_subscriber: NewSubscriber,
    base_url: &str,
    subscription_token: &str,
) -> Result<(), reqwest::Error> {
    // Build a confirmation link with a dynamic root
    let confirmation_link = format!(
        "{}/subscriptions/confirm?subscription_token={}",
        base_url, subscription_token
    );
    let plain_body = format!(
        "Welcome to our newsletter!\nVisit {} to confirm your subscription.",
        confirmation_link
    );
    let html_body = format!(
        "Welcome to our newsletter!<br />\
        Click <a href=\"{}\">here</a> to confirm your subscription.",
        confirmation_link
    );

    email_client
        .send_email(new_subscriber.email, "Welcome!", &html_body, &plain_body)
        .await
}

fn generate_subscription_token() -> String {
    let mut rng = thread_rng();
    std::iter::repeat_with(|| rng.sample(Alphanumeric))
        .map(char::from)
        .take(25)
        .collect()
}

#[tracing::instrument(
    name = "Store subscription token in the database",
    skip(subscription_token, transaction)
)]
pub async fn store_token(
    transaction: &mut Transaction<'_, Postgres>,
    subscriber_id: Uuid,
    subscription_token: &str,
) -> Result<(), StoreTokenError> {
    let query = sqlx::query!(
        r#"INSERT INTO subscription_tokens (subscription_token, subscriber_id) VALUES ($1, $2)"#,
        subscription_token,
        subscriber_id
    );

    transaction
        .execute(query)
        .await
        .map_err(|e| StoreTokenError(e))?;

    Ok(())
}

/**
 * Iterates the whole chain of errors that led to the failure we're trying to print
 */
fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    writeln!(f, "{}\n", e)?;
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Caused by:\n\t{}", cause)?;
        current = e.source();
    }

    Ok(())
}

// No longer needed because we're using thiserror::Error
// impl std::fmt::Display for SubscriberError {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         match self {
//             SubscriberError::ValidationError(e) => write!(f, "{}", e),
//             SubscriberError::StoreTokenError(_) => write!(
//                 f,
//                 "Failed to store the confirmation token for a new subscriber."
//             ),
//             SubscriberError::SendEmailError(_) => write!(f, "Failed to send a confirmation email."),
//             SubscriberError::PoolError(_) => {
//                 write!(f, "Failed to acquire a Postgres connection from the pool")
//             }
//             SubscriberError::InsertSubscriberError(_) => {
//                 write!(f, "Failed to insert new subscriber in the database.")
//             }
//             SubscriberError::TransactionCommitError(_) => {
//                 write!(
//                     f,
//                     "Failed to commit SQL transaction to store a new subscriber."
//                 )
//             }
//         }
//     }
// }

// impl std::error::Error for SubscriberError {
//     fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
//         match self {
//             // &str does not implement 'Error'. We'd consider it the root cause
//             SubscriberError::ValidationError(_) => None,
//             SubscriberError::StoreTokenError(e) => Some(e),
//             SubscriberError::SendEmailError(e) => Some(e),
//             SubscriberError::PoolError(e) => Some(e),
//             SubscriberError::InsertSubscriberError(e) => Some(e),
//             SubscriberError::TransactionCommitError(e) => Some(e),
//         }
//     }
// }
