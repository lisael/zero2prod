use std::net::TcpListener;

use uuid::Uuid;

use sqlx::{Connection, Executor, PgConnection, PgPool};

use once_cell::sync::Lazy;

use zero2prod::configuration::{get_configuration, DatabaseSettings};
use zero2prod::startup::run;
use zero2prod::telemetry::{get_subscriber, init_subscriber};

pub struct TestApp {
    address: String,
    pg_pool: PgPool,
}

#[actix_rt::test]
async fn health_check_works() {
    // Arrange
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    // Act
    let response = client
        .get(&format!("{}{}", &app.address, "/health_check"))
        .send()
        .await
        .expect("Failed to execute request.");

    // Assert
    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
}

#[actix_rt::test]
async fn subscribe_returns_a_200_for_valid_form_data() {
    // Arrange
    let app = spawn_app().await;

    let client = reqwest::Client::new();

    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    // Act
    let response = client
        .post(&format!("{}/subscriptions", &app.address))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .expect("Failed to execute request.");
    // Assert
    assert_eq!(200, response.status().as_u16());

    let saved = sqlx::query!("SELECT email, name FROM subscriptions",)
        .fetch_one(&app.pg_pool)
        .await
        .expect("Failed to fetch saved subscription.");

    assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    assert_eq!(saved.name, "le guin");
}

#[actix_rt::test]
async fn subscribe_returns_a_400_when_data_is_missing() {
    // Arrange
    let app = spawn_app().await;
    let client = reqwest::Client::new();
    let test_cases = vec![
        ("name=le%20guin", "missing the email"),
        ("email=ursula_le_guin%40gmail.com", "missing the name"),
        ("", "missing both name and email"),
    ];

    for (invalid_body, error_message) in test_cases {
        // Act
        let response = client
            .post(&format!("{}/subscriptions", &app.address))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(invalid_body)
            .send()
            .await
            .expect("Failed to execute request.");

        // Assert
        assert_eq!(
            400,
            response.status().as_u16(),
            // Additional customised error message on test failure
            "The API did not fail with 400 Bad Request when the payload was {}.",
            error_message
        );
    }
}

static TRACING: Lazy<()> = Lazy::new(|| {
    let mut configuration = get_configuration().expect("failed to load configuration");
    configuration.application_name = "zero2prod_tests".to_owned();
    if std::env::var("TEST_LOG").is_ok() {
        let subscriber = get_subscriber(&configuration, std::io::stdout);
        init_subscriber(subscriber);
    } else {
        let subscriber = get_subscriber(&configuration, std::io::sink);
        init_subscriber(subscriber);
    };
});

// Launch our application in the background
pub async fn spawn_app() -> TestApp {
    Lazy::force(&TRACING);
    let mut configuration = get_configuration().expect("failed to load configuration");
    configuration.application_name = "zero2prod_tests".to_owned();
    let listener = TcpListener::bind("127.0.0.1:0").expect("Can't allocate a random port");
    let port = listener.local_addr().unwrap().port();
    configuration.database.database_name = Uuid::new_v4().to_string();
    let pg_pool = configure_database(&configuration.database).await;
    let server = run(listener, pg_pool.clone()).expect("Failed to bind address");
    let _ = tokio::spawn(server);
    TestApp {
        address: format!("http://127.0.0.1:{}", port),
        pg_pool,
    }
}

pub async fn configure_database(settings: &DatabaseSettings) -> PgPool {
    let mut connection = PgConnection::connect(&settings.connection_string_wo_db())
        .await
        .expect("Failed to connect without db");
    connection
        .execute(&*format!(
            r#"CREATE DATABASE "{}";"#,
            settings.database_name
        ))
        .await
        .expect("Failed to create test database");
    let pool = PgPool::connect(&settings.connection_string())
        .await
        .expect("Failed to connect to Postgres.");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to migrate the test database");
    pool
}
