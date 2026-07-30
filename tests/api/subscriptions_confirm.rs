use base64::{Engine, engine::general_purpose};
use mail_parser::MessageParser;
use reqwest::Url;
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
    let email_request_body: serde_json::Value =
        serde_json::from_slice(&email_request.body).unwrap();
    let email_message_wire = general_purpose::URL_SAFE_NO_PAD
        .decode(email_request_body["raw"].as_str().unwrap())
        .unwrap();

    let message = MessageParser::default()
        .parse(&email_message_wire)
        .expect("Failed to extract original message from MIME-format");

    let get_link = |s: &str| {
        let links: Vec<_> = linkify::LinkFinder::new()
            .links(s)
            .filter(|l| *l.kind() == linkify::LinkKind::Url)
            .collect();
        assert_eq!(
            links.len(),
            1,
            "did not find exactly 1 link in the provided slice"
        );
        links[0].as_str().to_owned()
    };

    let raw_link = get_link(&message.body_html(0).unwrap());
    let mut link = Url::parse(&raw_link).unwrap();
    assert_eq!(link.host_str().unwrap(), "127.0.0.1");

    link.set_port(Some(app.port)).unwrap();

    let response = reqwest::get(link).await.unwrap();

    assert_eq!(
        response.status().as_u16(),
        200,
        "/subscriptions/confirm did not respond with a 200"
    );
}
