use sea_query::{Expr, ExprTrait, IntoColumnRef, SimpleExpr, extension::postgres::PgBinOper};

use crate::TsQuery;

/// Full-text search operators on a `tsvector` column.
pub trait FtsExprTrait: IntoColumnRef + Sized {
    /// `tsvector @@ tsquery` — whether this column matches the query.
    ///
    /// Takes the [`TsQuery`] by reference so the same query value can also be
    /// passed to [`TsQuery::rank`], which is what keeps matching and ranking
    /// in agreement.
    fn fts_matches(self, query: &TsQuery) -> SimpleExpr {
        Expr::col(self).binary(PgBinOper::Matches, SimpleExpr::from(query))
    }
}

impl<T: IntoColumnRef> FtsExprTrait for T {}
