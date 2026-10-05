use actix_web::{HttpResponse, ResponseError, web};
use askama::Template;
use chrono::Utc;
use rand::distr::Alphanumeric;
use rand::{RngExt, rng};
use sqlx::{Executor, PgPool, PgTransaction};
use std::error::Error;
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

struct StoreTokenError(sqlx::Error);

impl From<sqlx::Error> for StoreTokenError {
    fn from(value: sqlx::Error) -> Self {
        Self(value)
    }
}

impl std::fmt::Debug for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl std::fmt::Display for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "A database error was encountered while \
            trying to store a subscription token."
        )
    }
}

impl std::error::Error for StoreTokenError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
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

impl ResponseError for StoreTokenError {}

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
) -> Result<HttpResponse, actix_web::Error> {
    let Ok(new_subscriber) = form.0.try_into() else {
        return Ok(HttpResponse::BadRequest().finish());
    };

    let Ok(mut transaction) = db_pool.begin().await else {
        return Ok(HttpResponse::InternalServerError().finish());
    };

    let Ok(subscriber_id) = insert_subscriber(&new_subscriber, &mut transaction).await else {
        return Ok(HttpResponse::InternalServerError().finish());
    };

    let subscription_token = store_token(subscriber_id, &mut transaction).await?;

    if send_confirmation_email(
        &email_client,
        new_subscriber,
        &base_url.0,
        &subscription_token,
    )
    .await
    .is_err()
    {
        return Ok(HttpResponse::InternalServerError().finish());
    };

    if transaction.commit().await.is_err() {
        return Ok(HttpResponse::InternalServerError().finish());
    }

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
) -> Result<(), Box<dyn Error>> {
    let confirmation_link = format!(
        "{}/subscriptions/confirm?subscription_token={}",
        base_url, subscription_token
    );
    let html_body = ConfirmationTemplateHtml {
        confirmation_link: &confirmation_link,
    }
    .render()?;
    let txt_body = ConfirmationTemplateTxt {
        confirmation_link: &confirmation_link,
    }
    .render()?;

    email_client
        .send_email(
            new_subscriber.email,
            "Confirmation Email",
            &html_body,
            &txt_body,
        )
        .await
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
) -> Result<String, StoreTokenError> {
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
