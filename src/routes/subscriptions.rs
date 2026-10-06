use actix_web::http::StatusCode;
use actix_web::{HttpResponse, ResponseError, web};
use anyhow::Context;
use askama::Template;
use chrono::Utc;
use rand::distr::Alphanumeric;
use rand::{RngExt, rng};
use sqlx::{Executor, PgPool, PgTransaction};
use uuid::Uuid;

use crate::{
    domain::{NewSubscriber, SubscriberEmail, SubscriberName},
    email_client::EmailClient,
    startup::ApplicationBaseUrl,
};

#[derive(serde::Deserialize)]
pub struct FormData {
    name: String,
    email: String,
}

#[derive(Template)]
#[template(path = "confirmation.html")]
struct ConfirmationTemplateHtml<'a> {
    confirmation_link: &'a str,
}

#[derive(Template)]
#[template(path = "confirmation.txt")]
struct ConfirmationTemplateTxt<'a> {
    confirmation_link: &'a str,
}

fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    writeln!(f, "{}\n", e)?;
    let mut curr = e.source();
    while let Some(cause) = curr {
        writeln!(f, "Caused by:\n\t{}", cause)?;
        curr = cause.source();
    }
    Ok(())
}

#[derive(thiserror::Error)]
pub enum SubscribeError {
    #[error("{0}")]
    ValidationError(String),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for SubscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for SubscribeError {
    fn status_code(&self) -> actix_web::http::StatusCode {
        match self {
            SubscribeError::ValidationError(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl TryFrom<FormData> for NewSubscriber {
    type Error = String;

    fn try_from(form: FormData) -> Result<Self, Self::Error> {
        let (name, email) = (
            SubscriberName::parse(form.name)?,
            SubscriberEmail::parse(form.email)?,
        );
        Ok(NewSubscriber { email, name })
    }
}

fn generate_subscription_token() -> String {
    rng()
        .sample_iter(Alphanumeric)
        .take(25)
        .map(char::from)
        .collect()
}

#[tracing::instrument(
    name = "Adding a new subscriber",
    skip(form, db_pool, email_client, base_url),
    fields(
        subscriber_email = %form.email,
        subscriber_name = %form.name
    )
)]
pub async fn subscribe(
    form: web::Form<FormData>,
    db_pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    base_url: web::Data<ApplicationBaseUrl>,
) -> Result<HttpResponse, SubscribeError> {
    let new_subscriber = form.0.try_into().map_err(SubscribeError::ValidationError)?;
    let mut transaction = db_pool
        .begin()
        .await
        .context("Failed to obtain a connection from the database pool.")?;
    let subscriber_id = insert_subscriber(&new_subscriber, &mut transaction)
        .await
        .context("Failed to insert a new subscriber.")?;
    let subscription_token = store_token(subscriber_id, &mut transaction)
        .await
        .context("Failed to store the subscription token.")?;

    send_confirmation_email(
        &email_client,
        new_subscriber,
        &base_url.0,
        &subscription_token,
    )
    .await
    .context("Failed to send confirmation mail.")?;

    transaction
        .commit()
        .await
        .context("Failed to commit transaction to store the new subscriber.")?;

    Ok(HttpResponse::Ok().finish())
}

#[tracing::instrument(
    name = "Sending confirmation email",
    skip(email_client, new_subscriber, base_url)
)]
async fn send_confirmation_email(
    email_client: &EmailClient,
    new_subscriber: NewSubscriber,
    base_url: &str,
    subscription_token: &str,
) -> Result<(), anyhow::Error> {
    let confirmation_link = format!(
        "{}/subscriptions/confirm?subscription_token={}",
        base_url, subscription_token
    );
    let html_body = ConfirmationTemplateHtml {
        confirmation_link: &confirmation_link,
    }
    .render()
    .context("Failed to render HTML template.")?;
    let txt_body = ConfirmationTemplateTxt {
        confirmation_link: &confirmation_link,
    }
    .render()
    .context("Failed to render plain text template.")?;

    Ok(email_client
        .send_email(
            new_subscriber.email,
            "Confirmation Email",
            &html_body,
            &txt_body,
        )
        .await
        .context("Transport error while trying to send email.")?)
}

#[tracing::instrument(
    name = "Saving new subscriber details in the database",
    skip(data, transaction)
)]
async fn insert_subscriber(
    data: &NewSubscriber,
    transaction: &mut PgTransaction<'_>,
) -> Result<Uuid, sqlx::Error> {
    let subscriber_id = Uuid::new_v4();
    let maybe_id = sqlx::query_scalar!(
        "SELECT id FROM subscriptions WHERE email = $1",
        data.email.as_ref()
    );

    if let Some(subscriber_id) = maybe_id
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|e| {
            tracing::error!("Failed to execute query: {:?}", e);
            e
        })?
    {
        return Ok(subscriber_id);
    };

    let query = sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at, status)
        VALUES ($1, $2, $3, $4, 'pending_confirmation')
        "#,
        subscriber_id,
        data.email.as_ref(),
        data.name.as_ref(),
        Utc::now()
    );
    transaction.execute(query).await.map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;
    Ok(subscriber_id)
}

#[tracing::instrument(name = "Store subscription token in the database", skip(transaction))]
async fn store_token(
    subscriber_id: Uuid,
    transaction: &mut PgTransaction<'_>,
) -> Result<String, sqlx::Error> {
    let maybe_token = sqlx::query_scalar!(
        "SELECT subscription_token FROM subscription_tokens WHERE subscriber_id = $1",
        subscriber_id
    );
    if let Some(subscription_token) = maybe_token
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|e| {
            tracing::error!("Failed to execute query: {:?}", e);
            e
        })?
    {
        return Ok(subscription_token);
    };
    let subscription_token = generate_subscription_token();
    let query = sqlx::query!(
        r#"INSERT INTO subscription_tokens (subscription_token, subscriber_id)
        VALUES ($1, $2)"#,
        subscription_token,
        subscriber_id
    );
    transaction.execute(query).await.map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;
    Ok(subscription_token)
}
