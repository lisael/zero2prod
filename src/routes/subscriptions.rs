use std::convert::{TryFrom, TryInto};

use chrono::Utc;

use uuid::Uuid;

use actix_web::{web, HttpResponse};

use sqlx::PgPool;

use crate::domain::{NewSubscriber, SubscriberEmail, SubscriberName};

#[derive(serde::Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}

impl TryFrom<FormData> for NewSubscriber {
    type Error = String;
    fn try_from(value: FormData) -> Result<Self, Self::Error> {
        let name = SubscriberName::parse(value.name)?;
        let email = SubscriberEmail::parse(value.email)?;
        Ok(Self { email, name })
    }
}

#[tracing::instrument(
    name = "Adding a new subscriber",
    skip(form, pg_pool),
    fields(
        subscriber_email = %form.email,
        subscriber_name= %form.name
    )
)]
pub async fn subscribe(form: web::Form<FormData>, pg_pool: web::Data<PgPool>) -> HttpResponse {
    let new = match form.0.try_into() {
        Ok(new) => new,
        Err(_) => return HttpResponse::BadRequest().finish(),
    };
    match insert_subscriber(&new, &pg_pool).await {
        Ok(_) => {
            tracing::info!("{} subsciption details saved", new.email.as_ref());
            HttpResponse::Ok().finish()
        }
        Err(e) => {
            tracing::error!("Failed to execute query: {:?}", e);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[tracing::instrument(
    name = "Saving new subscriber details in the database",
    skip(new, pg_pool)
)]
pub async fn insert_subscriber(new: &NewSubscriber, pg_pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at)
        VALUES ($1, $2, $3, $4)
        "#,
        Uuid::new_v4(),
        new.email.as_ref(),
        new.name.as_ref(),
        Utc::now()
    )
    .execute(pg_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;
    Ok(())
}
