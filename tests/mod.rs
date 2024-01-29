use surrealdb::{engine::remote::ws::{Client, Ws}, opt::auth::Root, Surreal};
use surrealdb_migrations::Migrations;

#[tokio::test]
async fn create_migration_table_if_not_exists() {
    let client: Surreal<Client> = Surreal::new::<Ws>("127.0.0.1:8000").await.unwrap();
    client.signin(Root {
        username: "root",
        password: "root",
    })
    .await.unwrap();
    client.use_ns("system").use_db("system").await.unwrap();

    
    Migrations::new(&client).run().await.unwrap();
}
