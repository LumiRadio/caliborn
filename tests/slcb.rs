use caliborn::{
    entities,
    repositories::AlwaysCloneableConnection,
    services::slcb::{self, StreamlabsRecord},
};
use migration::MigratorTrait;
use rstest::{fixture, rstest};
use sea_orm::{ActiveValue, DatabaseConnection, EntityTrait};
use testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;

#[fixture]
async fn db() -> (AlwaysCloneableConnection, ContainerAsync<Postgres>) {
    let container = testcontainers_modules::postgres::Postgres::default()
        .with_tag("12")
        .start()
        .await
        .expect("Failed to start postgres container");

    let conn: DatabaseConnection = sea_orm::Database::connect(&format!(
        "postgres://postgres:postgres@{}:{}/postgres",
        container.get_host().await.unwrap(),
        container.get_host_port_ipv4(5432).await.unwrap()
    ))
    .await
    .expect("Failed to connect to postgres");

    migration::Migrator::up(&conn, None)
        .await
        .expect("Failed to run migrations");

    (AlwaysCloneableConnection::from(conn), container)
}

async fn insert_user(
    conn: &AlwaysCloneableConnection,
    id: i64,
    boonbucks: i32,
    watched_time: i64,
    migrated: bool,
) {
    entities::users::Entity::insert(entities::users::ActiveModel {
        id: ActiveValue::set(id),
        boonbucks: ActiveValue::set(boonbucks),
        watched_time: ActiveValue::set(watched_time),
        migrated: ActiveValue::set(migrated),
        ..Default::default()
    })
    .exec(&**conn)
    .await
    .unwrap();
}

async fn link_youtube(conn: &AlwaysCloneableConnection, user_id: i64, channel_id: &str) {
    entities::connected_youtube_accounts::Entity::insert(
        entities::connected_youtube_accounts::ActiveModel {
            user_id: ActiveValue::set(user_id),
            youtube_channel_id: ActiveValue::set(channel_id.into()),
            youtube_channel_name: ActiveValue::set("Channel".into()),
            ..Default::default()
        },
    )
    .exec(&**conn)
    .await
    .unwrap();
}

#[rstest]
#[awt]
#[tokio::test]
async fn import_inserts_then_updates(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;

    let records = vec![
        StreamlabsRecord {
            username: "alice".into(),
            user_id: Some("UCalice".into()),
            hours: 10,
            points: 100,
        },
        StreamlabsRecord {
            username: "bob".into(),
            user_id: None,
            hours: 5,
            points: 50,
        },
    ];

    let s1 = slcb::import_records(&conn, &records, false).await.unwrap();
    assert_eq!(s1.inserted, 2);
    assert_eq!(s1.updated, 0);

    // Re-import with updated values — should update both rows.
    let updated = vec![
        StreamlabsRecord {
            username: "alice".into(),
            user_id: Some("UCalice".into()),
            hours: 20,
            points: 200,
        },
        StreamlabsRecord {
            username: "bob".into(),
            user_id: None,
            hours: 7,
            points: 70,
        },
    ];
    let s2 = slcb::import_records(&conn, &updated, false).await.unwrap();
    assert_eq!(s2.inserted, 0);
    assert_eq!(s2.updated, 2);
}

#[rstest]
#[awt]
#[tokio::test]
async fn dry_run_imports_nothing(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    use sea_orm::PaginatorTrait;
    let (conn, _c) = db;
    let records = vec![StreamlabsRecord {
        username: "alice".into(),
        user_id: Some("UCalice".into()),
        hours: 10,
        points: 100,
    }];
    let summary = slcb::import_records(&conn, &records, true).await.unwrap();
    assert_eq!(summary.inserted, 1);

    let count = entities::slcb_currency::Entity::find()
        .count(&*conn)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[rstest]
#[awt]
#[tokio::test]
async fn match_imports_balances_for_linked_user(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 1, 10, 0, false).await;
    link_youtube(&conn, 1, "UC1").await;
    slcb::import_records(
        &conn,
        &[StreamlabsRecord {
            username: "alice".into(),
            user_id: Some("UC1".into()),
            hours: 5,
            points: 100,
        }],
        false,
    )
    .await
    .unwrap();

    let summary = slcb::match_youtube_links(&conn).await.unwrap();
    assert_eq!(summary.matched, 1);

    let user = entities::users::Entity::find_by_id(1_i64)
        .one(&*conn)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(user.watched_time, 5 * 3600);
    assert_eq!(user.boonbucks, 10 + 100);
    assert!(user.migrated);
}

#[rstest]
#[awt]
#[tokio::test]
async fn match_skips_already_migrated(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 1, 0, 0, true).await;
    link_youtube(&conn, 1, "UC1").await;
    slcb::import_records(
        &conn,
        &[StreamlabsRecord {
            username: "alice".into(),
            user_id: Some("UC1".into()),
            hours: 5,
            points: 100,
        }],
        false,
    )
    .await
    .unwrap();

    let summary = slcb::match_youtube_links(&conn).await.unwrap();
    assert_eq!(summary.matched, 0);
    assert_eq!(summary.already_migrated, 1);

    let user = entities::users::Entity::find_by_id(1_i64)
        .one(&*conn)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(user.watched_time, 0);
    assert_eq!(user.boonbucks, 0);
}

#[rstest]
#[awt]
#[tokio::test]
async fn match_is_idempotent(#[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>)) {
    let (conn, _c) = db;
    insert_user(&conn, 1, 0, 0, false).await;
    link_youtube(&conn, 1, "UC1").await;
    slcb::import_records(
        &conn,
        &[StreamlabsRecord {
            username: "alice".into(),
            user_id: Some("UC1".into()),
            hours: 5,
            points: 100,
        }],
        false,
    )
    .await
    .unwrap();

    let s1 = slcb::match_youtube_links(&conn).await.unwrap();
    assert_eq!(s1.matched, 1);
    let s2 = slcb::match_youtube_links(&conn).await.unwrap();
    assert_eq!(s2.matched, 0);
    assert_eq!(s2.already_migrated, 1);

    let user = entities::users::Entity::find_by_id(1_i64)
        .one(&*conn)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(user.watched_time, 5 * 3600);
    assert_eq!(user.boonbucks, 100);
}

#[rstest]
#[awt]
#[tokio::test]
async fn match_handles_link_without_slcb_row(
    #[future] db: (AlwaysCloneableConnection, ContainerAsync<Postgres>),
) {
    let (conn, _c) = db;
    insert_user(&conn, 1, 0, 0, false).await;
    link_youtube(&conn, 1, "UC_no_slcb").await;

    let summary = slcb::match_youtube_links(&conn).await.unwrap();
    assert_eq!(summary.matched, 0);
    assert_eq!(summary.no_slcb_row, 1);
}
