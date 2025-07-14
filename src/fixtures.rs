use std::fs::File;

use sea_orm::{
    ActiveModelTrait, ConnectionTrait, DatabaseBackend, DatabaseConnection, EntityName,
    EntityTrait, IntoActiveModel, Statement,
};
use serde_json::Value;

// code yoinked from loco-rs

/// Seeds the database with data from a YAML file.
///
/// ## Example data
///
/// ```yaml
/// - id: 1
///   name: John Doe
///   email: john@example.com
/// - id: 2
///   name: Jane Doe
///   email: jane@example.com
/// ```
#[tracing::instrument(skip(db))]
pub async fn seed<A>(db: &DatabaseConnection, path: &str) -> anyhow::Result<()>
where
    <<A as ActiveModelTrait>::Entity as EntityTrait>::Model: IntoActiveModel<A>,
    for<'de> <<A as ActiveModelTrait>::Entity as EntityTrait>::Model: serde::de::Deserialize<'de>,
    A: ActiveModelTrait + Send + Sync,
    sea_orm::Insert<A>: Send + Sync,
    <A as ActiveModelTrait>::Entity: EntityName,
{
    let seed_data: Vec<Value> = serde_yaml::from_reader(File::open(path)?)?;

    for row in seed_data {
        let model = A::from_json(row)?;
        <A::Entity as EntityTrait>::insert(model).exec(db).await?;
    }

    let table_name = A::Entity::default().table_name().to_string();
    let db_backend = db.get_database_backend();

    reset_auto_increment(db_backend, &table_name, db).await?;

    Ok(())
}

/// Checks if the specified table has an 'id' column.
///
/// This function checks if the specified table has an 'id' column, which is a
/// common primary key column. It supports `Postgres`, `SQLite`, and `MySQL`
/// database backends.
///
/// # Arguments
///
/// - `db`: A reference to the `DatabaseConnection`.
/// - `db_backend`: A reference to the `DatabaseBackend`.
/// - `table_name`: The name of the table to check.
///
/// # Returns
///
/// A `Result` containing a `bool` indicating whether the table has an 'id'
/// column.
async fn has_id_column(
    db: &DatabaseConnection,
    db_backend: &DatabaseBackend,
    table_name: &str,
) -> anyhow::Result<bool> {
    // First check if 'id' column exists
    let result = match db_backend {
        DatabaseBackend::Postgres => {
            let query = format!(
                "SELECT EXISTS (
              SELECT 1
              FROM information_schema.columns
              WHERE table_name = '{table_name}'
              AND column_name = 'id'
          )"
            );
            let result = db
                .query_one(Statement::from_string(DatabaseBackend::Postgres, query))
                .await?;
            result.is_some_and(|row| row.try_get::<bool>("", "exists").unwrap_or(false))
        }
        DatabaseBackend::Sqlite => {
            let query = format!(
                "SELECT COUNT(*) as count
          FROM pragma_table_info('{table_name}')
          WHERE name = 'id'"
            );
            let result = db
                .query_one(Statement::from_string(DatabaseBackend::Sqlite, query))
                .await?;
            result.is_some_and(|row| row.try_get::<i32>("", "count").unwrap_or(0) > 0)
        }
        DatabaseBackend::MySql => {
            return Err(anyhow::anyhow!(
                "Unsupported database backend `MySQL` for id column check"
            ));
        }
    };

    Ok(result)
}

/// Checks whether the specified table has an auto-increment 'id' column.
///
/// # Returns
///
/// A `Result` containing a `bool` indicating whether the table has an
/// auto-increment 'id' column.
async fn is_auto_increment(
    db: &DatabaseConnection,
    db_backend: &DatabaseBackend,
    table_name: &str,
) -> anyhow::Result<bool> {
    let result = match db_backend {
        DatabaseBackend::Postgres => {
            let query = format!(
                "SELECT pg_get_serial_sequence('{table_name}', 'id') IS NOT NULL as is_serial"
            );
            let result = db
                .query_one(Statement::from_string(DatabaseBackend::Postgres, query))
                .await?;
            result.is_some_and(|row| row.try_get::<bool>("", "is_serial").unwrap_or(false))
        }
        DatabaseBackend::Sqlite => {
            let query =
                format!("SELECT sql FROM sqlite_master WHERE type='table' AND name='{table_name}'");
            let result = db
                .query_one(Statement::from_string(DatabaseBackend::Sqlite, query))
                .await?;
            result.is_some_and(|row| {
                row.try_get::<String>("", "sql")
                    .is_ok_and(|sql| sql.to_lowercase().contains("autoincrement"))
            })
        }
        DatabaseBackend::MySql => {
            return Err(anyhow::anyhow!(
                "Unsupported database backend `MySQL` for auto-increment check"
            ));
        }
    };
    Ok(result)
}

/// Function to reset auto-increment
/// # Errors
/// Returns error if it fails
pub async fn reset_auto_increment(
    db_backend: DatabaseBackend,
    table_name: &str,
    db: &DatabaseConnection,
) -> anyhow::Result<()> {
    // Check if 'id' column exists
    let has_id_column = has_id_column(db, &db_backend, table_name).await?;
    if !has_id_column {
        return Ok(());
    }
    // Check if 'id' column is auto-increment
    let is_auto_increment = is_auto_increment(db, &db_backend, table_name).await?;
    if !is_auto_increment {
        return Ok(());
    }

    match db_backend {
        DatabaseBackend::Postgres => {
            let query_str = format!(
                "SELECT setval(pg_get_serial_sequence('{table_name}', 'id'), COALESCE(MAX(id), 0) \
                 + 1, false) FROM {table_name}"
            );
            db.execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                &query_str,
                vec![],
            ))
            .await?;
        }
        DatabaseBackend::Sqlite => {
            let query_str = format!(
                "UPDATE sqlite_sequence SET seq = (SELECT MAX(id) FROM {table_name}) WHERE name = \
                 '{table_name}'"
            );
            db.execute(Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                &query_str,
                vec![],
            ))
            .await?;
        }
        DatabaseBackend::MySql => {
            return Err(anyhow::anyhow!(
                "Unsupported database backend `MySQL` for auto-increment reset"
            ));
        }
    }
    Ok(())
}
