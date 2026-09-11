use axum::{
    Router,
    extract::State,
    routing::{delete, get, post},
};
use shared_constants::permissions::PERM_USE_BOT;

use crate::{
    AppState, ServiceRegistry,
    dtos::{
        Query,
        error::{CalibornResult, ErrorResponse},
        page::{Page, PaginationParams},
        songs::{SearchParams, SongDto, SongListDto, SongRequest, SongWithCooldownInfo},
    },
    services::auth::{AuthenticatedUser, authenticate},
};

#[utoipa::path(
    post,
    path = "/songs/request",
    params(SongRequest),
    responses(
        (status = 200, description = "Song queued; includes the user and song cooldowns now in effect", body = SongWithCooldownInfo),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "Caller lacks the `use_bot` permission", body = ErrorResponse),
        (status = 404, description = "No song matches the given file hash", body = ErrorResponse),
        (status = 429, description = "User or song is still on cooldown", body = ErrorResponse),
        (status = 500, description = "An internal server error occurred", body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn request_song(
    AuthenticatedUser(actor): AuthenticatedUser,
    State(registry): State<ServiceRegistry>,
    Query(song_request): Query<SongRequest>,
) -> CalibornResult<SongWithCooldownInfo> {
    let user_service = registry.user_service();
    let song_service = registry.song_service();

    user_service.get_user(actor.user_id()).await?;
    user_service
        .user_has_permission(actor.user_id(), PERM_USE_BOT)
        .await?;

    let song_with_cooldown = song_service
        .request_song(actor.user_id(), &song_request.file_hash)
        .await?;
    Ok(song_with_cooldown)
}

#[utoipa::path(
    get,
    path = "/songs/queue",
    responses(
        (status = 200, description = "Pending song requests, in play order", body = SongListDto),
        (status = 500, description = "An internal server error occurred", body = ErrorResponse),
    )
)]
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

#[utoipa::path(
    get,
    path = "/songs/history",
    params(PaginationParams),
    responses(
        (status = 200, description = "Recently played songs, newest first", body = Page<SongDto>),
        (status = 500, description = "An internal server error occurred", body = ErrorResponse),
    )
)]
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

#[utoipa::path(
    get,
    path = "/songs/search",
    params(SearchParams, PaginationParams),
    responses(
        (status = 200, description = "Full-text search results over the song library", body = Page<SongDto>),
        (status = 500, description = "An internal server error occurred", body = ErrorResponse),
    )
)]
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

#[utoipa::path(
    get,
    path = "/songs/favourites",
    params(SearchParams, PaginationParams),
    responses(
        (status = 200, description = "Search results restricted to the caller's favourites", body = Page<SongDto>),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "Caller lacks the `use_bot` permission", body = ErrorResponse),
        (status = 500, description = "An internal server error occurred", body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn search_favourite_songs(
    State(registry): State<ServiceRegistry>,
    AuthenticatedUser(actor): AuthenticatedUser,
    Query(params): Query<SearchParams>,
    Query(pagination): Query<PaginationParams>,
) -> CalibornResult<Page<SongDto>> {
    let song_service = registry.song_service();
    let user_service = registry.user_service();

    user_service.get_user(actor.user_id()).await?;
    user_service
        .user_has_permission(actor.user_id(), PERM_USE_BOT)
        .await?;

    song_service
        .search_favourite_songs(actor.user_id(), &params, &pagination)
        .await
        .map_err(Into::into)
        .map(|page| page.map(|song| song.into()))
}

#[utoipa::path(
    get,
    path = "/songs/current",
    responses(
        (status = 200, description = "The song currently on air", body = SongDto),
        (status = 404, description = "Nothing has been played yet", body = ErrorResponse),
        (status = 500, description = "An internal server error occurred", body = ErrorResponse),
    )
)]
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

#[utoipa::path(
    post,
    path = "/songs/favourite",
    params(("song_id" = String, Query, description = "File hash of the song to favourite")),
    responses(
        (status = 200, description = "Song added to the caller's favourites"),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "Caller lacks the `use_bot` permission", body = ErrorResponse),
        (status = 404, description = "No song matches the given file hash", body = ErrorResponse),
        (status = 500, description = "An internal server error occurred", body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn mark_song_as_favourite(
    State(registry): State<ServiceRegistry>,
    AuthenticatedUser(actor): AuthenticatedUser,
    Query(song_id): Query<String>,
) -> CalibornResult<()> {
    let song_service = registry.song_service();
    let user_service = registry.user_service();

    user_service.get_user(actor.user_id()).await?;
    user_service
        .user_has_permission(actor.user_id(), PERM_USE_BOT)
        .await?;

    song_service
        .mark_song_as_favourite(actor.user_id(), &song_id)
        .await
        .map_err(Into::into)
}

#[utoipa::path(
    delete,
    path = "/songs/favourite",
    params(("song_id" = String, Query, description = "File hash of the song to un-favourite")),
    responses(
        (status = 200, description = "Song removed from the caller's favourites"),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "Caller lacks the `use_bot` permission", body = ErrorResponse),
        (status = 404, description = "No song matches the given file hash", body = ErrorResponse),
        (status = 500, description = "An internal server error occurred", body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn unmark_song_as_favourite(
    State(registry): State<ServiceRegistry>,
    AuthenticatedUser(actor): AuthenticatedUser,
    Query(song_id): Query<String>,
) -> CalibornResult<()> {
    let song_service = registry.song_service();
    let user_service = registry.user_service();

    user_service.get_user(actor.user_id()).await?;
    user_service
        .user_has_permission(actor.user_id(), PERM_USE_BOT)
        .await?;

    song_service
        .unmark_song_as_favourite(actor.user_id(), &song_id)
        .await
        .map_err(Into::into)
}

#[utoipa::path(
    post,
    path = "/songs/favourite/current",
    responses(
        (status = 200, description = "The song currently on air was added to the caller's favourites"),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "Caller lacks the `use_bot` permission", body = ErrorResponse),
        (status = 404, description = "Nothing has been played yet", body = ErrorResponse),
        (status = 500, description = "An internal server error occurred", body = ErrorResponse),
    ),
    security(("user_jwt" = []), ("user_api_key" = []))
)]
#[axum::debug_handler]
pub async fn mark_currently_playing_song_as_favourite(
    State(registry): State<ServiceRegistry>,
    AuthenticatedUser(actor): AuthenticatedUser,
) -> CalibornResult<()> {
    let song_service = registry.song_service();
    let user_service = registry.user_service();

    user_service.get_user(actor.user_id()).await?;
    user_service
        .user_has_permission(actor.user_id(), PERM_USE_BOT)
        .await?;

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
