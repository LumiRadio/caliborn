use std::str::FromStr;

use sea_orm::{ColumnTrait, Iden, IdenStatic, Iterable};
use sea_query::{BinOper, Expr, Func, SimpleExpr};

#[derive(Iden)]
#[iden = "unnest"]
pub struct Unnest;

#[derive(Iden)]
#[iden = "to_tsvector"]
pub struct ToTsVector;

#[derive(Iden)]
#[iden = "to_tsquery"]
pub struct ToTsQuery;

pub trait TsQueryTrait: IdenStatic + Iterable + FromStr + ColumnTrait {
    fn full_text_search<T>(&self, query: T) -> SimpleExpr
    where
        T: Into<String>,
    {
        Expr::col((self.entity_name(), *self)).binary(
            BinOper::Custom("@@"),
            Func::cust(ToTsQuery).arg(Expr::val(query.into())),
        )
    }
}

impl<T> TsQueryTrait for T where T: IdenStatic + Iterable + FromStr + ColumnTrait {}
