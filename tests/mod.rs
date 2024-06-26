use surrealdb::{engine::remote::ws::{Client, Ws}, opt::auth::Root, Surreal};


#[derive(rust_embed::RustEmbed)]
#[folder = "tests/migrations"]
struct MigrationFiles;

#[derive(rust_embed::RustEmbed)]
#[folder = "tests/schema"]
struct SchemaFiles;

/// Start the server with the following command:
/// ```bash
/// podman run -u root --rm -p 8000:8000 -v ./surrealdb/data:/surrealdb/data surrealdb/surrealdb:v1.1.1 start --auth --user root --pass root file:/surrealdb/data/mydatabase.db
/// ```
/// Connect to the server with the following command:
/// ```bash
///  podman run -it --rm --network=host surrealdb/surrealdb:v1.1.1 sql --endpoint ws://0.0.0.0:8000 -u root -p root
/// ```
#[tokio::test]
async fn create_migration_table_if_not_exists() {
    tracing_subscriber::fmt::init();
    let client: Surreal<Client> = Surreal::new::<Ws>("127.0.0.1:8000").await.unwrap();
    client.signin(Root {
        username: "root",
        password: "root",
    })
    .await.unwrap();
    client.use_ns("system").use_db("system").await.unwrap();

    surrealdb_migration_engine::run::<MigrationFiles,SchemaFiles>(&client).await.unwrap();
}  
