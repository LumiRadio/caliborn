use sea_orm_migration::{prelude::*, sea_orm::Statement};
use shared_constants::permissions::{
    PERM_MANAGE_COOLDOWNS, PERM_MANAGE_PERMISSIONS, PERM_MANAGE_SLCB,
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        let new_perms = [
            PERM_MANAGE_PERMISSIONS,
            PERM_MANAGE_COOLDOWNS,
            PERM_MANAGE_SLCB,
        ];

        for p in new_perms {
            let (sql, values) = Query::insert()
                .into_table(Permissions::Table)
                .columns([
                    Permissions::Name,
                    Permissions::Description,
                    Permissions::BuiltIn,
                ])
                .values_panic([p.name.into(), p.description.into(), p.built_in.into()])
                .on_conflict(
                    OnConflict::column(Permissions::Name)
                        .do_nothing()
                        .to_owned(),
                )
                .build(PostgresQueryBuilder);
            db.execute(Statement::from_sql_and_values(
                db.get_database_backend(),
                sql,
                values,
            ))
            .await?;
        }

        for p in new_perms {
            let (sql, values) = Query::insert()
                .into_table(RolePermissions::Table)
                .columns([RolePermissions::Role, RolePermissions::Permission])
                .values_panic(["admin".into(), p.name.into()])
                .on_conflict(
                    OnConflict::columns([RolePermissions::Role, RolePermissions::Permission])
                        .do_nothing()
                        .to_owned(),
                )
                .build(PostgresQueryBuilder);
            db.execute(Statement::from_sql_and_values(
                db.get_database_backend(),
                sql,
                values,
            ))
            .await?;
        }

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Permissions {
    Table,
    Name,
    Description,
    BuiltIn,
}

#[derive(DeriveIden)]
enum RolePermissions {
    Table,
    Role,
    Permission,
}
