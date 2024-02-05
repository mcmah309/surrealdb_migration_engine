# surrealdb_migration_engine

[![crates.io](https://img.shields.io/crates/v/surrealdb_migration_engine.svg)](https://crates.io/crates/surrealdb_migration_engine)
[![License: MIT](https://img.shields.io/badge/license-MIT-purple.svg)](https://opensource.org/licenses/MIT)

Super simple yet power migration engine for surreal db. All you need to get it working is the following
```rust
#[derive(rust_embed::RustEmbed)]
#[folder = "migrations"]
struct MigrationFiles;

#[derive(rust_embed::RustEmbed)]
#[folder = "schema"]
struct SchemaFiles;

async fn main() {
    // create surealdb `client`

    SurrealdbMigrationEngine::run::<MigrationFiles,SchemaFiles>(&client).await?;

    // the rest of your code
}
```
## How It Works
`surrealdb_migration_engine` works on two concepts `migrations` and `schema`. Migrations are queries (changes) to an apply to an existing schema. Schemas are queries that set up the db structure. Schemas and migrations reside in their own directory with each file being numbered in order e.g. `0001_add_age_to_user_table.surql`. Each of these directories is compiled with your binary with the help of the `rust_embed` crate. This means that the appropriate migrations or schema creation will happen at runtime. All migrations and schema changes are done in a single transaction, so if one fails, they all fail.

`surrealdb_migration_engine` creates a `migrations` table inside your database to track which migrations have ran. The logic flow works like this. 
- If the migrations table does not exist, run only the `schema` files and enter all current `migration` files into the migration table.
- If the migrations table does exist, run any `migration` files that are not in the `migration` table

Very simple but extremely flexible!

## Uses
- Include `surrealdb_migration_engine` in your application so whenever you run your application, the schema is always up to date.
- Include `surrealdb_migration_engine` in a barebones executable that runs the necessary migrations or schema creation before an application starts.
