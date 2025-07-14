use caliborn::{
    dtos::songs::SearchParams,
    entities,
    pg_extension::{ToTsQuery, ToTsVector, TsQueryTrait, Unnest},
};
use sea_orm::{EntityTrait, Iterable, QueryFilter, QueryTrait};
use sea_query::{
    Alias, BinOper, CommonTableExpression, Expr, Func, PostgresQueryBuilder, Query, SelectStatement,
};

fn main() {
    let search_params = SearchParams {
        query: "how do i live without you (bunny back in the box)".to_string(),
        artist: None,
        album: None,
        title: None,
    };

    let mut query = entities::songs::Entity::find()
        .inner_join(entities::songs_fulltext::Entity)
        .filter(
            entities::songs_fulltext::Column::Tsvector
                .full_text_search(search_params.as_ts_query()),
        )
        .build(sea_orm::DatabaseBackend::Postgres);

    println!("{}", query.to_string());
}
