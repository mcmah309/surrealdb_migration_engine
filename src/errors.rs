use error_set::error_set;

error_set! {
    /// Errors related to migrations and schema creation. If more detail is needed, enable the "tracing" feature on the crate.
    MigrationsError = {
        CannotLoadFile,
        FileNumbering,
        FileNameMalformed,
        MigrationFileDbMismatch,
        MigrationFileInDbNotLongerExists,
        InfoForDbTablesNotAnObject,
        InfoForDbDoesNotContainTables,
        InfoForDbNotAnObject,
        InfoForDbHasNoData,
        Surrealdb(surrealdb::Error),
    };
}