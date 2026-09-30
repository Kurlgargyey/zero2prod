use actix_web::{HttpResponse, web};
use chrono::Utc;
use rand::distr::Alphanumeric;
use rand::{RngExt, rng};
use sqlx::PgPool;
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
) -> HttpResponse {
    let Ok(new_subscriber) = form.0.try_into() else {
        return HttpResponse::BadRequest().finish();
    };

    let Ok(subscriber_id) = insert_subscriber(&new_subscriber, &db_pool).await else {
        return HttpResponse::InternalServerError().finish();
    };

    let subscription_token = generate_subscription_token();
    if store_token(subscriber_id, &subscription_token, &db_pool)
        .await
        .is_err()
    {
        return HttpResponse::InternalServerError().finish();
    }

    if send_confirmation_email(
        &email_client,
        new_subscriber,
        &base_url.0,
        &subscription_token,
    )
    .await
    .is_err()
    {
        return HttpResponse::InternalServerError().finish();
    };

    HttpResponse::Ok().finish()
}

#[tracing::instrument(
    name = "Sending confirmation email",
    skip(email_client, new_subscriber, base_url)
)]
pub async fn send_confirmation_email(
    email_client: &EmailClient,
    new_subscriber: NewSubscriber,
    base_url: &str,
    subscription_token: &str,
) -> Result<(), Box<dyn Error>> {
    let confirmation_link = format!(
        "{}/subscriptions/confirm?subscription_token={}",
        base_url, subscription_token
    );
    email_client
        .send_email(
            new_subscriber.email,
            "Confirmation Email",
            &format!(
                "Please confirm your email<br />\
                <a href=\"{}\">Confirm</a>",
                confirmation_link
            ),
            &format!("Please confirm your email\n{}", confirmation_link),
        )
        .await
}

#[tracing::instrument(
    name = "Saving new subscriber details in the database",
    skip(data, db_pool)
)]
pub async fn insert_subscriber(
    data: &NewSubscriber,
    db_pool: &PgPool,
) -> Result<Uuid, sqlx::Error> {
    let subscriber_id = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at, status)
        VALUES ($1, $2, $3, $4, 'pending_confirmation')
        "#,
        subscriber_id,
        data.email.as_ref(),
        data.name.as_ref(),
        Utc::now()
    )
    .execute(db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;
    Ok(subscriber_id)
}

#[tracing::instrument(
    name = "Store subscription token in the database",
    skip(subscription_token, db_pool)
)]
pub async fn store_token(
    subscriber_id: Uuid,
    subscription_token: &str,
    db_pool: &PgPool,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"INSERT INTO subscription_tokens (subscription_token, subscriber_id)
        VALUES ($1, $2)"#,
        subscription_token,
        subscriber_id
    )
    .execute(db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;
    Ok(())
}
