use caliborn::{dtos::songs::SearchParams, entities};
use pg_fts::{FtsExprTrait, TsConfig, TsQuery};
use sea_orm::{EntityTrait, QueryFilter, QueryTrait};

fn main() {
    let search_params = SearchParams {
        query: Some("how do i live without you (bunny back in the box)".to_string()),
        artist: None,
        album: None,
        title: None,
        ..Default::default()
    };

    let ts_query = TsQuery::websearch_prefix(TsConfig::ENGLISH, search_params.query.unwrap());

    let query = entities::songs::Entity::find()
        .inner_join(entities::songs_fulltext::Entity)
        .filter(
            (
                entities::songs_fulltext::Entity,
                entities::songs_fulltext::Column::Tsvector,
            )
                .fts_matches(&ts_query),
        )
        .build(sea_orm::DatabaseBackend::Postgres);

    println!("{}", query.to_string());
}
