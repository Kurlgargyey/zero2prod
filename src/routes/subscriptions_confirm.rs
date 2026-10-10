use actix_web::{HttpResponse, ResponseError, http::StatusCode, web};
use anyhow::Context;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::error_chain_fmt;

#[derive(serde::Deserialize)]
pub struct Parameters {
    subscription_token: String,
}

#[derive(thiserror::Error)]
#[error("Error while trying to confirm a new subscriber.")]
pub enum ConfirmError {
    #[error("Invalid subscription token.")]
    InvalidToken,
    #[error("Token has no associated subscriber.")]
    NoSubscriberWaiting,
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for ConfirmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for ConfirmError {
    fn status_code(&self) -> actix_web::http::StatusCode {
        match self {
            ConfirmError::InvalidToken => StatusCode::BAD_REQUEST,
            ConfirmError::NoSubscriberWaiting => StatusCode::UNAUTHORIZED,
            ConfirmError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[tracing::instrument(
    name = "Fetch subscriber ID from database.",
    skip(pool, subscription_token)
)]
async fn get_subscriber_id_from_token(
    pool: &PgPool,
    subscription_token: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    let result = sqlx::query!(
        "SELECT subscriber_id FROM subscription_tokens WHERE subscription_token = $1",
        subscription_token
    )
    .fetch_optional(pool)
    .await?;
    Ok(result.map(|r| r.subscriber_id))
}

async fn validate_subscription_token(subscription_token: &str) -> Result<(), ConfirmError> {
    if subscription_token.len() != 25 || !subscription_token.is_ascii() {
        return Err(ConfirmError::InvalidToken);
    };
    Ok(())
}

#[tracing::instrument(name = "Mark subscriber as confirmed", skip(id, pool))]
async fn confirm_subscriber(id: Uuid, pool: &PgPool) -> Result<(), ConfirmError> {
    sqlx::query!(
        r#"UPDATE subscriptions SET status = 'confirmed' WHERE id = $1"#,
        id
    )
    .execute(pool)
    .await
    .context("Failed to set subscription status to 'confirmed'.")?;

    sqlx::query!(
        "DELETE FROM subscription_tokens WHERE subscriber_id = $1",
        id
    )
    .execute(pool)
    .await
    .context("Failed to delete used confirmation token from database.")?;

    Ok(())
}

#[tracing::instrument(name = "Confirm a pending subscriber", skip(parameters, db_pool))]
pub async fn confirm(
    parameters: web::Query<Parameters>,
    db_pool: web::Data<PgPool>,
) -> Result<HttpResponse, ConfirmError> {
    validate_subscription_token(&parameters.subscription_token).await?;
    let maybe_id = get_subscriber_id_from_token(&db_pool, &parameters.subscription_token)
        .await
        .context("Failed to get subscriber ID from token.")?;
    let Some(id) = maybe_id else {
        return Err(ConfirmError::NoSubscriberWaiting);
    };
    confirm_subscriber(id, &db_pool)
        .await
        .context("Failed to confirm subscriber.")?;
    Ok(HttpResponse::Ok().finish())
}
