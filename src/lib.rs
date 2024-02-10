use std::borrow::Cow;

use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use surrealdb::{engine::remote::ws::Client, Surreal};

pub struct SurrealdbMigrationEngine;

impl SurrealdbMigrationEngine {
    /// If the `migrations` table does not exist, run only the schema files, create a `migrations` table and add all of the current migration files to the table.
    /// If the `migrations` table does exist, run any migration files that are not in the `migrations` table and insert those migrations in the `migrations` table.
    pub async fn run<MigrationFiles, SchemaFiles>(
        client: &Surreal<Client>,
    ) -> Result<(), MigrationsError>
    where
        MigrationFiles: rust_embed::RustEmbed,
        SchemaFiles: rust_embed::RustEmbed,
    {
        if create_migration_table_and_schema_if_not_exists::<MigrationFiles, SchemaFiles>(&client)
            .await?
        {
            return Ok(()); // No migrations to run
        }
        run_any_new_migrations::<MigrationFiles, SchemaFiles>(&client).await?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MigrationsError {
    #[error("key '{0}' is not an object in query result '{1}'")]
    QueryResultNotAnObject(String, String),
    #[error("key '{0}' not found in query result '{1}'")]
    QueryResultKeyNotFound(String, String),
    #[error("query result is empty for query '{0}")]
    QueryResultEmpty(String),
    #[error("File name '{0}' is malformed. Expected format e.g.: '0001_name.sursql'")]
    FileNameMalformed(String),
    #[error("Cannot load file '{0}'")]
    CannotLoadFile(String),
    #[error("{0}")]
    MigrationTableOrFileCorruption(String),
    #[error("{0}")]
    FileNumbering(String),
    #[error("Surrealdb error: '{0}'\n Additional info: '{1}'")]
    Surrealdb(surrealdb::Error, String),
    // #[error("Original query error: '{0}'\n Additional info: '{1}'")]
    // WrappedError(Box<dyn Error + Send + Sync>, String),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Migration {
    file_name: String,
    number: u32,
    date_ran: Option<surrealdb::sql::Datetime>,
}

#[derive(Debug)]
struct SqlFile {
    file_name: String,
    number: u32,
    sql: String,
}

/// Creates the migration table and schema if it does not exist.
/// Returns true if the table was created, false if it already existed.
async fn create_migration_table_and_schema_if_not_exists<MigrationFiles, SchemaFiles>(
    client: &Surreal<Client>,
) -> Result<bool, MigrationsError>
where
    MigrationFiles: rust_embed::RustEmbed,
    SchemaFiles: rust_embed::RustEmbed,
{
    let get_migration_db = r#"
INFO FOR DB;
    "#;

    let result: Vec<Value> = client
        .query(get_migration_db)
        .await
        .map_err(|e| MigrationsError::Surrealdb(e, format!("for query {}", get_migration_db)))?
        .take(0)
        .map_err(|e| MigrationsError::Surrealdb(e, format!("for query {}", get_migration_db)))?;

    let Some(db_info) = result.get(0) else {
        return Err(MigrationsError::QueryResultEmpty(
            get_migration_db.to_string(),
        ));
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

    let existing_migrations_to_insert: Vec<Migration> = migrations
        .into_iter()
        .map(|migration| Migration {
            file_name: migration.file_name,
            number: migration.number,
            date_ran: None,
        })
        .collect();

    let mut query = client
        .query("BEGIN TRANSACTION;")
        .query(&create_schema_sql)
        .query(
            r#"
        DEFINE TABLE migrations SCHEMAFULL;

        DEFINE FIELD fileName ON TABLE migrations TYPE string;
        DEFINE FIELD number ON TABLE migrations TYPE int;
        DEFINE FIELD dateRan ON TABLE migrations TYPE option<datetime>;
        "#,
        );

    for (index, migration) in existing_migrations_to_insert.iter().enumerate() {
        query = query
            .query(format!("INSERT INTO migrations $migration{};", index))
            .bind((format!("migration{}", index), migration));
    }

    query
        .query("COMMIT TRANSACTION;")
        .await
        .map_err(|e| MigrationsError::Surrealdb(e, "".into()))?
        .check()
        .map_err(|e| MigrationsError::Surrealdb(e, "".into()))?;

    Ok(true)
}

async fn run_any_new_migrations<MigrationFiles, SchemaFiles>(
    client: &Surreal<Client>,
) -> Result<(), MigrationsError>
where
    MigrationFiles: rust_embed::RustEmbed,
    SchemaFiles: rust_embed::RustEmbed,
{
    let sql = r#"
SELECT * FROM migrations;
    "#;

    let db_migrations: Vec<Migration> = client
        .query(sql)
        .await
        .map_err(|e| MigrationsError::Surrealdb(e, format!("for query {}", sql)))?
        .take(0)
        .map_err(|e| MigrationsError::Surrealdb(e, format!("for query {}", sql)))?;

    let mut file_migrations = get_sql_files::<MigrationFiles>().await?;

    for db_migration in db_migrations.iter() {
        let (index, migration_file) = file_migrations
            .iter()
            .enumerate()
            .find(|(_index, migration_file)| migration_file.number == db_migration.number)
            .ok_or_else(|| {
                MigrationsError::MigrationTableOrFileCorruption(format!(
                    "Migration file not found for migration number '{}'. Original file name in db: '{}'",
                    db_migration.number,
                    db_migration.file_name
                ))
            })?;
        if db_migration.file_name != migration_file.file_name {
            return Err(MigrationsError::MigrationTableOrFileCorruption(format!(
                "Migration file name  '{}' does not match the file name in the database '{}'",
                migration_file.file_name, db_migration.file_name
            )));
        }
        file_migrations.remove(index);
    }

    if file_migrations.is_empty() {
        return Ok(()); // No migrations to run
    }

    let run_new_migrations = file_migrations
        .iter()
        .map(|migration| migration.sql.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let new_migration_table_entries = file_migrations.into_iter().map(|migration| Migration {
        file_name: migration.file_name,
        number: migration.number,
        date_ran: Some(surrealdb::sql::Datetime::from(Utc::now())),
    });

    let mut query = client
        .query("BEGIN TRANSACTION;")
        .query(&run_new_migrations);

    for (index, migration) in new_migration_table_entries.enumerate() {
        query = query
            .query(format!("INSERT INTO migrations $migration{};", index))
            .bind((format!("migration{}", index), migration));
    }

    query
        .query("COMMIT TRANSACTION;")
        .await
        .map_err(|e| MigrationsError::Surrealdb(e, "".into()))?
        .check()
        .map_err(|e| MigrationsError::Surrealdb(e, "".into()))?;

    Ok(())
}

async fn get_sql_files<F: rust_embed::RustEmbed>() -> Result<Vec<SqlFile>, MigrationsError> {
    let number_re = Regex::new(r"^\d+").unwrap();

    let mut number_and_file_name: Vec<(u32, Cow<str>)> = F::iter()
        .map(|file_name| {
            let migration_file_name = file_name.to_string();
            let migration_number = (|| {
                number_re
                    .captures(&file_name)?
                    .get(0)?
                    .as_str()
                    .parse::<u32>()
                    .ok()
            })()
            .ok_or_else(|| MigrationsError::FileNameMalformed(migration_file_name.clone()))?;
            Ok::<_, MigrationsError>((migration_number, file_name))
        })
        .collect::<Result<Vec<_>, MigrationsError>>()?;

    number_and_file_name.sort_by(|a, b| a.0.cmp(&b.0));

    // validate
    if let Some((number, name)) = number_and_file_name.first() {
        if number.to_owned() != 1 {
            return Err(MigrationsError::FileNumbering(format!(
                "First file number is not 1. File name: '{}'",
                name
            )));
        }
    }
    for (a, b) in number_and_file_name
        .iter()
        .zip(number_and_file_name.iter().skip(1))
    {
        if a.0 + 1 != b.0 {
            return Err(MigrationsError::FileNumbering(format!(
                "File numbers are not sequential or not one apart. File names: '{}' and '{}'",
                a.1, b.1
            )));
        }
    }

    let sql_files: Vec<SqlFile> = number_and_file_name
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

    Ok(sql_files)
}
