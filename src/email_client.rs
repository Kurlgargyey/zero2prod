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
    ) -> Result<(), reqwest::Error> {
        let url = self
            .base_url
            .join("/gmail/v1/users/me/messages/send")
            .expect("Invalid Email API subpath");
        let request_body = SendEmailRequest {
            from: self.sender.as_ref().to_owned(),
            to: recipient.as_ref().to_owned(),
            subject: subject.to_owned(),
            html_body: html_content.to_owned(),
            text_body: text_content.to_owned(),
        };
        let _builder = self
            .http_client
            .post(url)
            .header(
                "Authorization",
                format!("Bearer {}", self.auth_token.expose_secret()),
            )
            .json(&request_body)
            .send()
            .await?;
        Ok(())
    }
}

#[derive(serde::Serialize)]
struct SendEmailRequest {
    from: String,
    to: String,
    subject: String,
    html_body: String,
    text_body: String,
}

#[cfg(test)]
mod tests {
    use crate::domain::SubscriberEmail;
    use crate::email_client::EmailClient;

    use fake::Fake;
    use fake::faker::internet::en::SafeEmail;
    use fake::faker::lorem::en::{Paragraph, Sentence};
    use secrecy::SecretString;
    use wiremock::matchers::header_exists;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn send_email_fires_a_request_to_base_url() {
        let mock_server = MockServer::start().await;
        let sender = SubscriberEmail::parse(SafeEmail().fake()).unwrap();
        let email_client = EmailClient::new(mock_server.uri(), sender, SecretString::default());

        Mock::given(header_exists("Authorization"))
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
