use actix_web::{HttpResponse, ResponseError};

#[derive(thiserror::Error, Debug)]
#[error("Newsletter delivery failed.")]
pub enum NewsletterError {}

impl ResponseError for NewsletterError {}

pub async fn publish_newsletter() -> Result<HttpResponse, NewsletterError> {
    Ok(HttpResponse::Ok().finish())
}
