use error_set::error_set;

error_set! {
    /// Errors related to migrations and schema creation. If more detail is needed, enable the "tracing" feature on the crate.
    MigrationsError = {
        CannotLoadFile,
        /// Files are not numbered sequentially starteding from 1.
        FileNumbering,
        /// A file name does not follow the naming conventions outlined in the documentation.
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