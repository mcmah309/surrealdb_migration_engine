use std::error::Error;

use anyhow::bail;
use serde::Deserialize;
use serde_json::{map::Values, Value};
use surrealdb::{engine::remote::ws::Client, Surreal};

pub struct Migrations<'a> {
    client: &'a Surreal<Client>,
}

impl<'a> Migrations<'a> {
    pub fn new(client: &'a Surreal<Client>) -> Self {
        Self { client }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        create_migration_table_if_not_exists(self.client).await?;

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
enum MigrationsError {
    #[error("key '{0}' is not an object in query result '{1}'")]
    QueryResultNotAnObject(String, String),
    #[error("key '{0}' not found in query result '{1}'")]
    QueryResultKeyNotFound(String, String),
}


async fn create_migration_table_if_not_exists(
    client: &Surreal<Client>,
) -> anyhow::Result<()> {
    let get_migration_db = r#"
INFO FOR DB;
    "#;

    let result: Vec<Value> = client
        .query(get_migration_db)
        .await?
        .take(0)?;

    let Some(db_info) = result.get(0) else {
        bail!(
            "Unexpected result when getting info for the migrations database. Response was: {:?}",
            result
        );
    };

    let tables = db_info
        .as_object()
        .ok_or_else(|| MigrationsError::QueryResultNotAnObject("".into(), db_info.to_string()))?
        .get("tables")
        .ok_or_else(|| MigrationsError::QueryResultKeyNotFound("tables".into(),db_info.to_string()))?
        .as_object()
        .ok_or_else(|| MigrationsError::QueryResultNotAnObject("tables".into(), db_info.to_string()))?;

    let has_migrations_table = tables
        .get("migrations")
        .is_some();

    if has_migrations_table {
        return Ok(());
    }

    let sql = r#"
BEGIN TRANSACTION;

DEFINE TABLE migrations SCHEMAFULL;

DEFINE FIELD fileName ON TABLE migrations TYPE string;
DEFINE FIELD number ON TABLE migrations TYPE int;
DEFINE FIELD dateRan ON TABLE migrations TYPE datetime;

COMMIT TRANSACTION;
    "#;

    let _ = client
        .query(sql)
        .await?
        .check()?;

    Ok(())
}
