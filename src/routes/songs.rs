use axum::{
    Router,
    extract::State,
    routing::{delete, get, post},
};

use crate::{
    AppState, ServiceRegistry,
    dtos::{
        Query,
        error::CalibornResult,
        page::{Page, PaginationParams},
        songs::{SearchParams, SongDto, SongListDto, SongRequest, SongWithCooldownInfo},
    },
    services::{
        auth::{AuthenticatedUser, authenticate},
        songs::SongService,
        users::UserService,
    },
};

#[axum::debug_handler]
pub async fn request_song(
    AuthenticatedUser(actor): AuthenticatedUser,
    State(registry): State<ServiceRegistry>,
    Query(song_request): Query<SongRequest>,
) -> CalibornResult<SongWithCooldownInfo> {
    let user_service = registry.user_service();
    let song_service = registry.song_service();

    // ensure user
    user_service.get_user(actor.user_id()).await?;

    let song_with_cooldown = song_service
        .request_song(actor.user_id(), &song_request.file_hash)
        .await?;
    Ok(song_with_cooldown)
}

#[axum::debug_handler]
pub async fn get_request_queue(
    State(registry): State<ServiceRegistry>,
) -> CalibornResult<SongListDto> {
    let song_service = registry.song_service();
    song_service
        .get_request_queue()
        .await
        .map(SongListDto::from)
        .map_err(Into::into)
}

#[axum::debug_handler]
pub async fn get_song_history(
    State(registry): State<ServiceRegistry>,
    Query(pagination): Query<PaginationParams>,
) -> CalibornResult<Page<SongDto>> {
    let song_service = registry.song_service();
    song_service
        .get_song_history(&pagination)
        .await
        .map_err(Into::into)
}

#[axum::debug_handler]
pub async fn search_song(
    State(registry): State<ServiceRegistry>,
    Query(params): Query<SearchParams>,
    Query(pagination): Query<PaginationParams>,
) -> CalibornResult<Page<SongDto>> {
    let song_service = registry.song_service();
    song_service
        .search_song(&params, &pagination)
        .await
        .map_err(Into::into)
        .map(|page| page.map(|song| song.into()))
}

#[axum::debug_handler]
pub async fn search_favourite_songs(
    State(registry): State<ServiceRegistry>,
    AuthenticatedUser(actor): AuthenticatedUser,
    Query(params): Query<SearchParams>,
    Query(pagination): Query<PaginationParams>,
) -> CalibornResult<Page<SongDto>> {
    let song_service = registry.song_service();
    song_service
        .search_favourite_songs(actor.user_id(), &params, &pagination)
        .await
        .map_err(Into::into)
        .map(|page| page.map(|song| song.into()))
}

#[axum::debug_handler]
pub async fn get_currently_playing(
    State(registry): State<ServiceRegistry>,
) -> CalibornResult<SongDto> {
    let song_service = registry.song_service();
    song_service
        .get_currently_playing_song()
        .await
        .map_err(Into::into)
}

#[axum::debug_handler]
pub async fn mark_song_as_favourite(
    State(registry): State<ServiceRegistry>,
    AuthenticatedUser(actor): AuthenticatedUser,
    Query(song_id): Query<String>,
) -> CalibornResult<()> {
    let song_service = registry.song_service();
    song_service
        .mark_song_as_favourite(actor.user_id(), &song_id)
        .await
        .map_err(Into::into)
}

#[axum::debug_handler]
pub async fn unmark_song_as_favourite(
    State(registry): State<ServiceRegistry>,
    AuthenticatedUser(actor): AuthenticatedUser,
    Query(song_id): Query<String>,
) -> CalibornResult<()> {
    let song_service = registry.song_service();
    song_service
        .unmark_song_as_favourite(actor.user_id(), &song_id)
        .await
        .map_err(Into::into)
}

#[axum::debug_handler]
pub async fn mark_currently_playing_song_as_favourite(
    State(registry): State<ServiceRegistry>,
    AuthenticatedUser(actor): AuthenticatedUser,
) -> CalibornResult<()> {
    let song_service = registry.song_service();
    song_service
        .mark_currently_playing_song_as_favourite(actor.user_id())
        .await
        .map_err(Into::into)
}

pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/request", post(request_song))
        .route("/favourites", get(search_favourite_songs))
        .route("/favourite", post(mark_song_as_favourite))
        .route("/favourite", delete(unmark_song_as_favourite))
        .route(
            "/favourite/current",
            post(mark_currently_playing_song_as_favourite),
        )
        .layer(axum::middleware::from_fn_with_state(state, authenticate))
        .route("/queue", get(get_request_queue))
        .route("/history", get(get_song_history))
        .route("/search", get(search_song))
        .route("/current", get(get_currently_playing))
}
