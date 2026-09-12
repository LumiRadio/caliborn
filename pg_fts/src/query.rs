use sea_query::{
    BinOper, CaseStatement, Expr, ExprTrait, Func, FunctionCall, Iden, IntoColumnRef, SimpleExpr,
    extension::postgres::PgFunc,
};

use crate::TsConfig;

#[derive(Iden)]
#[iden = "to_tsquery"]
struct ToTsquery;

#[derive(Iden)]
#[iden = "websearch_to_tsquery"]
struct WebsearchToTsquery;

#[derive(Iden)]
#[iden = "plainto_tsquery"]
struct PlaintoTsquery;

#[derive(Iden)]
#[iden = "phraseto_tsquery"]
struct PhrasetoTsquery;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Plain,
    Phrase,
    Websearch,
    WebsearchPrefix,
}

/// A Postgres `tsquery`, built once and reused for both matching (`@@`) and
/// ranking (`ts_rank`).
///
/// Building it once is the point: `@@` and `ts_rank` given different tsqueries
/// will silently rank rows by a query they were not matched on.
#[derive(Debug, Clone)]
pub struct TsQuery {
    config: TsConfig,
    mode: Mode,
    input: String,
}

impl TsQuery {
    /// Websearch syntax (quoted phrases, `or`, `-negation`) with the final
    /// lexeme turned into a prefix, for type-ahead search.
    ///
    /// See `docs/superpowers/specs/2026-09-12-pg-fts-crate-design.md`: the
    /// query is normalised by Postgres first, rendered back to text, then
    /// re-parsed with `:*` appended. The text that is re-parsed is Postgres's
    /// own normalised output, never the raw user input. The `CASE` guards
    /// input that reduces to an empty tsquery, where appending `:*` would
    /// yield `':*'` and make `to_tsquery` raise a syntax error.
    pub fn websearch_prefix(config: TsConfig, input: impl Into<String>) -> Self {
        Self::new(config, Mode::WebsearchPrefix, input)
    }

    /// Websearch syntax, without prefix matching.
    pub fn websearch(config: TsConfig, input: impl Into<String>) -> Self {
        Self::new(config, Mode::Websearch, input)
    }

    /// Every word required, via `plainto_tsquery`.
    pub fn plain(config: TsConfig, input: impl Into<String>) -> Self {
        Self::new(config, Mode::Plain, input)
    }

    /// The words as an adjacent phrase, via `phraseto_tsquery`.
    pub fn phrase(config: TsConfig, input: impl Into<String>) -> Self {
        Self::new(config, Mode::Phrase, input)
    }

    fn new(config: TsConfig, mode: Mode, input: impl Into<String>) -> Self {
        Self {
            config,
            mode,
            input: input.into(),
        }
    }

    /// `ts_rank(vector, query)` — relevance of a row against this query.
    pub fn rank(&self, vector: impl IntoColumnRef) -> SimpleExpr {
        SimpleExpr::FunctionCall(PgFunc::ts_rank(
            Expr::col(vector),
            SimpleExpr::from(self),
        ))
    }

    /// `ts_rank_cd(vector, query)` — cover-density relevance, which accounts
    /// for how close the matched lexemes are to each other.
    pub fn rank_cd(&self, vector: impl IntoColumnRef) -> SimpleExpr {
        SimpleExpr::FunctionCall(PgFunc::ts_rank_cd(
            Expr::col(vector),
            SimpleExpr::from(self),
        ))
    }

    fn regconfig(&self) -> SimpleExpr {
        Expr::val(self.config.as_str().to_owned()).cast_as("regconfig")
    }

    fn call(&self, func: impl Iden + 'static) -> FunctionCall {
        Func::cust(func).args([self.regconfig(), Expr::val(self.input.clone())])
    }

    /// The normalised query rendered back to `text`, as
    /// `websearch_to_tsquery(...)::text`.
    fn websearch_as_text(&self) -> SimpleExpr {
        SimpleExpr::FunctionCall(self.call(WebsearchToTsquery)).cast_as("text")
    }
}

impl From<&TsQuery> for SimpleExpr {
    fn from(query: &TsQuery) -> Self {
        match query.mode {
            Mode::Plain => SimpleExpr::FunctionCall(query.call(PlaintoTsquery)),
            Mode::Phrase => SimpleExpr::FunctionCall(query.call(PhrasetoTsquery)),
            Mode::Websearch => SimpleExpr::FunctionCall(query.call(WebsearchToTsquery)),
            Mode::WebsearchPrefix => SimpleExpr::Case(Box::new(
                CaseStatement::new()
                    .case(
                        query.websearch_as_text().eq(""),
                        SimpleExpr::FunctionCall(query.call(WebsearchToTsquery)),
                    )
                    .finally(SimpleExpr::FunctionCall(
                        Func::cust(ToTsquery).args([
                            query.regconfig(),
                            query
                                .websearch_as_text()
                                .binary(BinOper::Custom("||"), Expr::val(":*")),
                        ]),
                    )),
            )),
        }
    }
}

impl From<TsQuery> for SimpleExpr {
    fn from(query: TsQuery) -> Self {
        SimpleExpr::from(&query)
    }
}
