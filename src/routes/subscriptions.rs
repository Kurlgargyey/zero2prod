use actix_web::{HttpResponse, web};
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    domain::{NewSubscriber, SubscriberEmail, SubscriberName},
    email_client::EmailClient,
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

#[tracing::instrument(
    name = "Adding a new subscriber",
    skip(form, db_pool, email_client),
    fields(
        subscriber_email = %form.email,
        subscriber_name = %form.name
    )
)]
pub async fn subscribe(
    form: web::Form<FormData>,
    db_pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
) -> HttpResponse {
    let Ok(new_subscriber) = form.0.try_into() else {
        return HttpResponse::BadRequest().finish();
    };

    if insert_subscriber(&new_subscriber, &db_pool).await.is_err() {
        return HttpResponse::InternalServerError().finish();
    };

    let confirmation_link = "https://confirmation.notreal.nah";

    if email_client
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
        .is_err()
    {
        return HttpResponse::InternalServerError().finish();
    };

    HttpResponse::Ok().finish()
}

#[tracing::instrument(
    name = "Saving new subscriber details in the database",
    skip(data, db_pool)
)]
pub async fn insert_subscriber(data: &NewSubscriber, db_pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at, status)
        VALUES ($1, $2, $3, $4, 'confirmed')
        "#,
        Uuid::new_v4(),
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
    Ok(())
}
