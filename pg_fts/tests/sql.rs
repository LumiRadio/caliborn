use pg_fts::TsConfig;

#[test]
fn ts_config_accepts_a_lowercase_identifier() {
    let config = TsConfig::new("german").expect("german is a valid config name");

    assert_eq!(config.as_str(), "german");
}

#[test]
fn ts_config_rejects_names_that_are_not_bare_identifiers() {
    for name in ["", "English", "en-gb", "english; drop table songs", "1english", "en glish"] {
        assert!(
            TsConfig::new(name).is_err(),
            "expected {name:?} to be rejected as a text search config name"
        );
    }
}

#[test]
fn ts_config_english_constant_is_english() {
    assert_eq!(TsConfig::ENGLISH.as_str(), "english");
}

use pg_fts::TsQuery;
use sea_query::{PostgresQueryBuilder, Query, SimpleExpr};

fn to_sql(expr: SimpleExpr) -> String {
    Query::select().expr(expr).to_string(PostgresQueryBuilder)
}

#[test]
fn websearch_prefix_guards_the_empty_tsquery_and_suffixes_the_last_lexeme() {
    let sql = to_sql(SimpleExpr::from(&TsQuery::websearch_prefix(
        TsConfig::ENGLISH,
        "lumi radio",
    )));

    assert_eq!(
        sql,
        "SELECT (CASE \
             WHEN (CAST(websearch_to_tsquery(CAST('english' AS regconfig), 'lumi radio') AS text) = '') \
             THEN websearch_to_tsquery(CAST('english' AS regconfig), 'lumi radio') \
             ELSE to_tsquery(CAST('english' AS regconfig), CAST(websearch_to_tsquery(CAST('english' AS regconfig), 'lumi radio') AS text) || ':*') \
         END)"
    );
}

use pg_fts::FtsExprTrait;
use sea_query::Iden;

#[derive(Iden)]
enum SongsFulltext {
    Table,
    Tsvector,
}

#[test]
fn fts_matches_renders_the_match_operator_against_a_qualified_column() {
    let query = TsQuery::websearch(TsConfig::ENGLISH, "lumi");

    let sql = to_sql((SongsFulltext::Table, SongsFulltext::Tsvector).fts_matches(&query));

    assert_eq!(
        sql,
        "SELECT \"songs_fulltext\".\"tsvector\" @@ \
         websearch_to_tsquery(CAST('english' AS regconfig), 'lumi')"
    );
}

#[test]
fn rank_scores_a_column_against_the_same_query() {
    let query = TsQuery::websearch(TsConfig::ENGLISH, "lumi");

    let sql = to_sql(query.rank((SongsFulltext::Table, SongsFulltext::Tsvector)));

    assert_eq!(
        sql,
        "SELECT TS_RANK(\"songs_fulltext\".\"tsvector\", \
         websearch_to_tsquery(CAST('english' AS regconfig), 'lumi'))"
    );
}

#[test]
fn plain_and_phrase_modes_use_their_own_constructors() {
    assert_eq!(
        to_sql(SimpleExpr::from(&TsQuery::plain(TsConfig::ENGLISH, "a b"))),
        "SELECT plainto_tsquery(CAST('english' AS regconfig), 'a b')"
    );
    assert_eq!(
        to_sql(SimpleExpr::from(&TsQuery::phrase(TsConfig::SIMPLE, "a b"))),
        "SELECT phraseto_tsquery(CAST('simple' AS regconfig), 'a b')"
    );
}

use pg_fts::{FtsColumn, gin_index, tsvector};
use sea_query::{ColumnDef, Table};

#[derive(Iden)]
enum Songs {
    Table,
    Title,
    Artist,
    Album,
}

#[test]
fn fts_column_builds_a_stored_generated_column_that_coalesces_null_sources() {
    let column = FtsColumn::new(SongsFulltext::Tsvector, TsConfig::ENGLISH)
        .source(Songs::Title)
        .source(Songs::Artist);

    let sql = Table::create()
        .table(Songs::Table)
        .col(ColumnDef::new(Songs::Album).string())
        .col(column.to_column_def())
        .to_string(PostgresQueryBuilder);

    assert!(
        sql.contains(
            "\"tsvector\" tsvector GENERATED ALWAYS AS \
             (to_tsvector(CAST('english' AS regconfig), coalesce(\"title\", '')) || \
             to_tsvector(CAST('english' AS regconfig), coalesce(\"artist\", ''))) STORED"
        ),
        "unexpected DDL: {sql}"
    );
}

#[test]
fn tsvector_is_the_postgres_tsvector_type() {
    let sql = Table::create()
        .table(Songs::Table)
        .col(ColumnDef::new_with_type(SongsFulltext::Tsvector, tsvector()))
        .to_string(PostgresQueryBuilder);

    assert!(sql.contains("\"tsvector\" tsvector"), "unexpected DDL: {sql}");
}

#[test]
fn gin_index_creates_a_gin_index_on_the_vector_column() {
    let sql = gin_index(
        "songs_fulltext_tsvector_idx",
        SongsFulltext::Table,
        SongsFulltext::Tsvector,
    )
    .to_string(PostgresQueryBuilder);

    assert_eq!(
        sql,
        "CREATE INDEX \"songs_fulltext_tsvector_idx\" ON \"songs_fulltext\" \
         USING GIN (\"tsvector\")"
    );
}
