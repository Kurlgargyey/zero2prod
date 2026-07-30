use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::helpers::spawn_app;

#[tokio::test]
async fn subscriptions_confirm_rejects_requests_without_a_token() {
    let app = spawn_app().await;

    let response = reqwest::get(format!("{}/subscriptions/confirm", app.address))
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 400);
}

#[tokio::test]
async fn subscriptions_confirm_accepts_valid_request() {
    let app = spawn_app().await;

    Mock::given(path("/gmail/v1/users/me/messages/send"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    app.post_subscriptions(body.into()).await;

    let email_request = &app.email_server.received_requests().await.unwrap()[0];

    let mut link = app.get_confirmation_links(email_request).plain_text;
    assert_eq!(link.host_str().unwrap(), "127.0.0.1");
    link.set_port(Some(app.port)).unwrap();

    let response = reqwest::get(link).await.unwrap();

    assert_eq!(
        response.status().as_u16(),
        200,
        "/subscriptions/confirm did not respond with a 200"
    );
}
