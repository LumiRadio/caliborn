use sea_query::{
    BinOper, ColumnDef, ColumnType, Expr, ExprTrait, Func, Iden, Index, IndexCreateStatement,
    IndexType, IntoIden, SimpleExpr,
};

use crate::TsConfig;

#[derive(Iden)]
#[iden = "to_tsvector"]
struct ToTsvector;

#[derive(Iden)]
#[iden = "coalesce"]
struct Coalesce;

/// The Postgres `tsvector` column type.
pub fn tsvector() -> ColumnType {
    ColumnType::custom("tsvector")
}

/// `CREATE INDEX <name> ON <table> USING GIN (<column>)`.
///
/// Without this index every `@@` match is a sequential scan.
pub fn gin_index(
    name: impl IntoIden,
    table: impl IntoIden,
    column: impl IntoIden,
) -> IndexCreateStatement {
    Index::create()
        .name(name.into_iden().to_string())
        .table(table)
        .col(column)
        .index_type(IndexType::FullText)
        .to_owned()
}

/// A stored generated `tsvector` column built from one or more source columns.
pub struct FtsColumn {
    name: sea_query::DynIden,
    config: TsConfig,
    sources: Vec<sea_query::DynIden>,
}

impl FtsColumn {
    /// A generated column named `name`, vectorised with `config`.
    pub fn new(name: impl IntoIden, config: TsConfig) -> Self {
        Self {
            name: name.into_iden(),
            config,
            sources: Vec::new(),
        }
    }

    /// Add a source column. Call once per column, in the order they should be
    /// concatenated.
    pub fn source(mut self, column: impl IntoIden) -> Self {
        self.sources.push(column.into_iden());
        self
    }

    /// The column definition: `tsvector GENERATED ALWAYS AS (...) STORED`.
    ///
    /// # Panics
    ///
    /// Panics if no source column was added; a generated column with no
    /// sources has no meaning.
    pub fn to_column_def(&self) -> ColumnDef {
        let mut columns = self.sources.iter().cloned().map(|source| self.vectorise(source));
        let first = columns
            .next()
            .expect("an fts column needs at least one source column");
        let expr = columns.fold(first, |acc, next| acc.binary(BinOper::Custom("||"), next));

        let mut def = ColumnDef::new_with_type(self.name.clone(), tsvector());
        def.generated(expr, true);
        def
    }

    /// `to_tsvector('<config>', coalesce("<column>", ''))`.
    ///
    /// The `coalesce` matters: in Postgres a NULL operand makes the whole
    /// concatenated tsvector NULL, so a single NULL source column would drop
    /// the row out of the index entirely.
    fn vectorise(&self, source: sea_query::DynIden) -> SimpleExpr {
        let coalesced = Func::cust(Coalesce).args([Expr::col(source), Expr::val("")]);

        SimpleExpr::FunctionCall(Func::cust(ToTsvector).args([
            Expr::val(self.config.as_str().to_owned()).cast_as("regconfig"),
            SimpleExpr::FunctionCall(coalesced),
        ]))
    }
}
