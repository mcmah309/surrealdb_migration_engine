use std::{borrow::Cow, error::Error};

use anyhow::bail;
use chrono::{DateTime, Utc};
use regex::Regex;
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
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
        if (create_migration_table_and_schema_if_not_exists(self.client).await?) {
            //run_new_migrations(self.client).await?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
enum MigrationsError {
    #[error("key '{0}' is not an object in query result '{1}'")]
    QueryResultNotAnObject(String, String),
    #[error("key '{0}' not found in query result '{1}'")]
    QueryResultKeyNotFound(String, String),
    #[error("File name '{0}' is malformed. Expected format e.g.: '0001_name.sursql'")]
    FileNameMalformed(String),
    #[error("Cannot load file '{0}'")]
    CannotLoadFile(String),
    #[error("Error running migration '{0}'")]
    WrappedError(#[from] Box<dyn Error + Send + Sync>),
}

#[derive(Debug, Deserialize, Serialize)]
struct Migration {
    file_name: String,
    number: i32,
    date_ran: Option<DateTime<chrono::Utc>>,
}

#[derive(Debug)]
struct SqlFile {
    file_name: String,
    number: i32,
    sql: String,
}

/// Creates the migration table and schema if it does not exist.
/// Returns true if the table was created, false if it already existed.
async fn create_migration_table_and_schema_if_not_exists(
    client: &Surreal<Client>,
) -> anyhow::Result<bool> {
    let get_migration_db = r#"
INFO FOR DB;
    "#;

    let result: Vec<Value> = client.query(get_migration_db).await?.take(0)?;

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
        .ok_or_else(|| {
            MigrationsError::QueryResultKeyNotFound("tables".into(), db_info.to_string())
        })?
        .as_object()
        .ok_or_else(|| {
            MigrationsError::QueryResultNotAnObject("tables".into(), db_info.to_string())
        })?;

    let has_migrations_table = tables.get("migrations").is_some();

    if has_migrations_table {
        return Ok(false);
    }

    let schemas = get_sql_files::<SchemaFiles>().await?;

    let create_schema_sql = schemas
        .iter()
        .map(|migration| migration.sql.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let migrations = get_sql_files::<MigrationFiles>().await?;

    let insert_existing_migrations_sql: String = migrations
        .into_iter()
        .map(|migration| Migration {
            file_name: migration.file_name,
            number: migration.number,
            date_ran: None,
        })
        .map(|migration| {
            Ok(format!(
                "INSERT INTO migrations {};",
                serde_json::to_string(&migration)?
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?
        .join("\n");

    let sql = format!(
        r#"
BEGIN TRANSACTION;

DEFINE TABLE migrations SCHEMAFULL;

DEFINE FIELD fileName ON TABLE migrations TYPE string;
DEFINE FIELD number ON TABLE migrations TYPE int;
DEFINE FIELD dateRan ON TABLE migrations TYPE datetime;

{}

{}

COMMIT TRANSACTION;
    "#,
        create_schema_sql, insert_existing_migrations_sql
    );

    let _ = client.query(sql).await?.check()?;

    Ok(true)
}

#[derive(RustEmbed)]
#[folder = "migrations"]
struct MigrationFiles;

#[derive(RustEmbed)]
#[folder = "schema"]
struct SchemaFiles;

async fn get_migrations_in_db(client: &Surreal<Client>) -> anyhow::Result<Vec<Migration>> {
    let sql = r#"
SELECT * FROM migrations;
    "#;

    let migrations: Vec<Migration> = client.query(sql).await?.take(0)?;

    Ok(migrations)
}

async fn get_sql_files<F: rust_embed::RustEmbed>() -> anyhow::Result<Vec<SqlFile>> {
    let number_re = Regex::new(r"^\d+").unwrap();

    let number_and_file_name: Vec<(i32, Cow<str>)> = F::iter()
        .map(|file_name| {
            let migration_file_name = file_name.to_string();
            let migration_number = (|| {
                number_re
                    .captures(&file_name)?
                    .get(0)?
                    .as_str()
                    .parse::<i32>()
                    .ok()
            })()
            .ok_or_else(|| MigrationsError::FileNameMalformed(migration_file_name.clone()))?;
            Ok::<_, MigrationsError>((migration_number, file_name))
        })
        .collect::<Result<Vec<_>, MigrationsError>>()?;

    let schemas: Vec<SqlFile> = number_and_file_name
        .into_iter()
        .map(|(number, file_name)| {
            Ok(SqlFile {
                file_name: file_name.to_string(),
                number: number,
                sql: String::from_utf8_lossy(
                    F::get(file_name.as_ref())
                        .ok_or_else(|| MigrationsError::CannotLoadFile(file_name.into_owned()))?
                        .data
                        .as_ref(),
                )
                .to_string(),
            })
        })
        .collect::<Result<Vec<_>, MigrationsError>>()?;

    Ok(schemas)
}
