use base64::{Engine, engine::general_purpose};
use mail_parser::MessageParser;
use reqwest::{Response, Url};
use secrecy::SecretString;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use std::sync::LazyLock;
use uuid::Uuid;
use wiremock::MockServer;
use zero2prod::{
    configuration::{DatabaseSettings, get_configuration},
    startup::{Application, get_connection_pool},
    telemetry::{get_subscriber, init_subscriber},
};

static TRACING: LazyLock<()> = LazyLock::new(|| {
    let default_filter_level = "debug".into();
    let subscriber_name = "test".into();

    if std::env::var("TEST_LOG").is_ok() {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::stdout);
        init_subscriber(subscriber);
    } else {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::sink);
        init_subscriber(subscriber);
    }
});

pub struct TestApp {
    pub address: String,
    pub pool: PgPool,
    pub email_server: MockServer,
    pub port: u16,
}

pub struct ConfirmationLinks {
    pub html: reqwest::Url,
    pub plain_text: reqwest::Url,
}

impl TestApp {
    pub async fn post_subscriptions(&self, body: String) -> Response {
        reqwest::Client::new()
            .post(&format!("{}/subscriptions", &self.address))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .expect("Failed to execute request")
    }

    pub fn get_confirmation_links(&self, email_request: &wiremock::Request) -> ConfirmationLinks {
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

        let html_link = Url::parse(&get_link(&message.body_html(0).unwrap())).unwrap();
        let text_link = Url::parse(&get_link(&message.body_text(0).unwrap())).unwrap();
        ConfirmationLinks {
            html: html_link,
            plain_text: text_link,
        }
    }
}

pub async fn spawn_app() -> TestApp {
    LazyLock::force(&TRACING);
    let email_server = MockServer::start().await;

    let mut configuration = get_configuration().expect("Failed to read configuration file");
    configuration.database.database_name = Uuid::new_v4().to_string();
    configuration.application.port = 0;
    configuration.email_client.base_url = email_server.uri();

    configure_database(&configuration.database).await;

    let app = Application::build(configuration.clone())
        .await
        .expect("Failed to build server");
    let port = app.port();
    let address = format!("http://127.0.0.1:{}", port);
    let _ = tokio::spawn(app.run_until_stopped());

    TestApp {
        address,
        pool: get_connection_pool(&configuration.database),
        email_server,
        port,
    }
}

async fn configure_database(config: &DatabaseSettings) -> PgPool {
    let maintenance_settings = DatabaseSettings {
        database_name: "postgres".to_string(),
        username: "postgres".to_string(),
        password: SecretString::new("password".to_string().into()),
        ..config.clone()
    };

    PgConnection::connect_with(&maintenance_settings.connection_options())
        .await
        .expect("Failed to connect to Postgres")
        .execute(format!(r#"CREATE DATABASE "{}";"#, config.database_name).as_str())
        .await
        .expect("Failed to create database");

    let connection_pool = PgPool::connect_with(config.connection_options())
        .await
        .expect("Failed to connect to Postgres");

    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to migrate database");

    connection_pool
}
