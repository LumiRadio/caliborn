use sea_orm::{ColumnTrait, EntityTrait, Iden, Iterable, PrimaryKeyToColumn};
use serde::Serialize;
use utoipa::ToSchema;

/// One column of an admin resource, as reported by
/// `GET /admin/crud/{resource}/_schema`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FieldMeta {
    pub name: String,
    pub column_type: String,
    pub nullable: bool,
    pub is_primary_key: bool,
}

pub fn schema_for<E: EntityTrait>() -> Vec<FieldMeta> {
    let pk_cols: Vec<E::Column> = E::PrimaryKey::iter().map(|pk| pk.into_column()).collect();

    E::Column::iter()
        .map(|col| {
            let def = col.def();
            FieldMeta {
                name: col.to_string(),
                column_type: format!("{:?}", def.get_column_type()),
                nullable: def.is_null(),
                is_primary_key: pk_cols.iter().any(|p| p.to_string() == col.to_string()),
            }
        })
        .collect()
}
