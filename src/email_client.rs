use std::{error::Error, str::FromStr};

use base64::{Engine, engine::general_purpose};
use email_message::{Address, Body, Mailbox, Message};
use secrecy::{ExposeSecret, SecretString};

use reqwest::Client;

use crate::domain::SubscriberEmail;

pub struct EmailClient {
    sender: SubscriberEmail,
    http_client: Client,
    base_url: reqwest::Url,
    auth_token: SecretString,
}

impl EmailClient {
    pub fn new(base_url: String, sender: SubscriberEmail, auth_token: SecretString) -> Self {
        let base_url = reqwest::Url::parse(&base_url).expect("Invalid email API base URL");
        Self {
            http_client: Client::new(),
            sender,
            base_url,
            auth_token,
        }
    }
    pub async fn send_email(
        &self,
        recipient: SubscriberEmail,
        subject: &str,
        html_content: &str,
        text_content: &str,
    ) -> Result<(), Box<dyn Error>> {
        let url = self
            .base_url
            .join("/gmail/v1/users/me/messages/send")
            .expect("Invalid Email API subpath");

        let body = Body::text_and_html(text_content, html_content);
        let to = Address::from_str(recipient.as_ref());
        let from = Mailbox::from_str(self.sender.as_ref())?;

        let message = email_message_wire::render_rfc822(
            &Message::builder(body)
                .to(to)
                .from_mailbox(from)
                .subject(subject)
                .build()?,
        )?;
        let message_b64 = general_purpose::URL_SAFE_NO_PAD.encode(message);
        let body = SendEmailRequest { raw: message_b64 };

        let _builder = self
            .http_client
            .post(url)
            .header(
                "Authorization",
                format!("Bearer {}", self.auth_token.expose_secret()),
            )
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await?;
        Ok(())
    }
}

#[derive(serde::Serialize)]
struct SendEmailRequest {
    raw: String,
}

#[cfg(test)]
mod tests {
    use crate::domain::SubscriberEmail;
    use crate::email_client::EmailClient;

    use base64::Engine;
    use base64::engine::general_purpose;
    use fake::Fake;
    use fake::faker::internet::en::SafeEmail;
    use fake::faker::lorem::en::{Paragraph, Sentence};
    use secrecy::SecretString;
    use wiremock::matchers::{header, header_exists, method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    struct SendEmailBodyMatcher;

    impl wiremock::Match for SendEmailBodyMatcher {
        fn matches(&self, request: &Request) -> bool {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&request.body) {
                let Some(data) = json.get("raw") else {
                    return false;
                };
                let Ok(message_wire) = general_purpose::URL_SAFE_NO_PAD.decode(
                    data.as_str()
                        .expect("key 'raw' does not contain b64 string"),
                ) else {
                    return false;
                };
                email_message_wire::parse_rfc822(&message_wire).is_ok()
            } else {
                false
            }
        }
    }

    #[tokio::test]
    async fn send_email_sends_the_expected_request() {
        let mock_server = MockServer::start().await;
        let sender = SubscriberEmail::parse(SafeEmail().fake()).unwrap();
        let email_client = EmailClient::new(mock_server.uri(), sender, SecretString::default());

        Mock::given(header_exists("Authorization"))
            .and(header("Accept", "application/json"))
            .and(header("Content-Type", "application/json"))
            .and(path("/gmail/v1/users/me/messages/send"))
            .and(method("POST"))
            .and(SendEmailBodyMatcher)
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        let subscriber_email = SubscriberEmail::parse(SafeEmail().fake()).unwrap();
        let subject: String = Sentence(1..2).fake();
        let content: String = Paragraph(1..10).fake();

        let _ = email_client
            .send_email(subscriber_email, &subject, &content, &content)
            .await;
    }
}
