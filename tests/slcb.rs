//! SLCB legacy-import / YouTube-match tests. Uses the shared `ScenarioEnv`
//! harness (see `common.rs`) for the DB + user/channel seeding helpers.

mod common;
use common::*;

use caliborn::{
    entities,
    services::slcb::{self, StreamlabsRecord},
};
use rstest::rstest;
use sea_orm::{EntityTrait, PaginatorTrait};

#[rstest]
#[awt]
#[tokio::test]
async fn import_inserts_then_updates(#[future] scenario: ScenarioEnv) {
    let env = scenario;

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

    let s1 = slcb::import_records(&env.conn, &records, false).await.unwrap();
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
    let s2 = slcb::import_records(&env.conn, &updated, false).await.unwrap();
    assert_eq!(s2.inserted, 0);
    assert_eq!(s2.updated, 2);
}

#[rstest]
#[awt]
#[tokio::test]
async fn dry_run_imports_nothing(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    let records = vec![StreamlabsRecord {
        username: "alice".into(),
        user_id: Some("UCalice".into()),
        hours: 10,
        points: 100,
    }];
    let summary = slcb::import_records(&env.conn, &records, true).await.unwrap();
    assert_eq!(summary.inserted, 1);

    let count = entities::slcb_currency::Entity::find()
        .count(&*env.conn)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[rstest]
#[awt]
#[tokio::test]
async fn match_imports_balances_for_linked_user(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user_full(1, 10, 0, false).await;
    env.link_youtube(1, "UC1").await;
    slcb::import_records(
        &env.conn,
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

    let summary = slcb::match_youtube_links(&env.conn).await.unwrap();
    assert_eq!(summary.matched, 1);

    let user = entities::users::Entity::find_by_id(1_i64)
        .one(&*env.conn)
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
async fn match_skips_already_migrated(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user_full(1, 0, 0, true).await;
    env.link_youtube(1, "UC1").await;
    slcb::import_records(
        &env.conn,
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

    let summary = slcb::match_youtube_links(&env.conn).await.unwrap();
    assert_eq!(summary.matched, 0);
    assert_eq!(summary.already_migrated, 1);

    let user = entities::users::Entity::find_by_id(1_i64)
        .one(&*env.conn)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(user.watched_time, 0);
    assert_eq!(user.boonbucks, 0);
}

#[rstest]
#[awt]
#[tokio::test]
async fn match_is_idempotent(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user_full(1, 0, 0, false).await;
    env.link_youtube(1, "UC1").await;
    slcb::import_records(
        &env.conn,
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

    let s1 = slcb::match_youtube_links(&env.conn).await.unwrap();
    assert_eq!(s1.matched, 1);
    let s2 = slcb::match_youtube_links(&env.conn).await.unwrap();
    assert_eq!(s2.matched, 0);
    assert_eq!(s2.already_migrated, 1);

    let user = entities::users::Entity::find_by_id(1_i64)
        .one(&*env.conn)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(user.watched_time, 5 * 3600);
    assert_eq!(user.boonbucks, 100);
}

#[rstest]
#[awt]
#[tokio::test]
async fn match_handles_link_without_slcb_row(#[future] scenario: ScenarioEnv) {
    let env = scenario;
    env.insert_user_full(1, 0, 0, false).await;
    env.link_youtube(1, "UC_no_slcb").await;

    let summary = slcb::match_youtube_links(&env.conn).await.unwrap();
    assert_eq!(summary.matched, 0);
    assert_eq!(summary.no_slcb_row, 1);
}
