use caliborn::{dtos::songs::SearchParams, entities, pg_extension::TsQueryTrait};
use sea_orm::{EntityTrait, QueryFilter, QueryTrait};

fn main() {
    let search_params = SearchParams {
        query: Some("how do i live without you (bunny back in the box)".to_string()),
        artist: None,
        album: None,
        title: None,
        ..Default::default()
    };

    let query = entities::songs::Entity::find()
        .inner_join(entities::songs_fulltext::Entity)
        .filter(
            entities::songs_fulltext::Column::Tsvector
                .full_text_search(search_params.as_ts_query().unwrap()),
        )
        .build(sea_orm::DatabaseBackend::Postgres);

    println!("{}", query.to_string());
}
