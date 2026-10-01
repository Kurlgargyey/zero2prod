use actix_web::{HttpResponse, web};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(serde::Deserialize)]
#[allow(dead_code)]
pub struct Parameters {
    subscription_token: String,
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
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;
    Ok(result.map(|r| r.subscriber_id))
}

#[tracing::instrument(name = "Mark subscriber as confirmed", skip(id, pool))]
async fn confirm_subscriber(id: Uuid, pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE subscriptions SET status = 'confirmed' WHERE id = $1"#,
        id
    )
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;

    sqlx::query!(
        "DELETE FROM subscription_tokens WHERE subscriber_id = $1",
        id
    )
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;

    Ok(())
}

#[tracing::instrument(name = "Confirm a pending subscriber", skip(parameters, db_pool))]
pub async fn confirm(
    parameters: web::Query<Parameters>,
    db_pool: web::Data<PgPool>,
) -> HttpResponse {
    let Ok(maybe_id) = get_subscriber_id_from_token(&db_pool, &parameters.subscription_token).await
    else {
        return HttpResponse::InternalServerError().finish();
    };
    let Some(id) = maybe_id else {
        return HttpResponse::Unauthorized().finish();
    };
    if confirm_subscriber(id, &db_pool).await.is_err() {
        return HttpResponse::InternalServerError().finish();
    };
    HttpResponse::Ok().finish()
}
