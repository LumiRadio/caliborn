use sea_orm::{QueryOrder, QuerySelect, prelude::*};

use crate::{
    dtos::page::{Page, PaginationParams},
    entities, generate_dtos,
    pg_extension::TsQueryTrait,
    repositories::{ApplyQueryFilter, BaseRepository, RepositoryError},
    vectorizer::to_tsvector,
};

/// A trait representing a repository for songs.
#[async_trait::async_trait]
pub trait SongRepositoryExt: Send + Sync + 'static {
    /// Delete a song by its file hash.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while deleting the
    /// song.
    async fn delete_by_hash(&self, file_hash: &str) -> Result<(), RepositoryError>;

    /// Prune songs that no longer exist on disk.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while pruning the
    /// songs.
    async fn prune(&self) -> Result<(), RepositoryError>;

    /// Find a song by its file hash.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while retrieving the
    /// song.
    async fn find_by_hash(
        &self,
        file_hash: &str,
    ) -> Result<Option<entities::songs::Model>, RepositoryError>;

    /// Count the total number of songs in the database.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while counting the
    /// songs.
    async fn count(&self) -> Result<u64, RepositoryError>;

    /// Find the songs that have been requested recently.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while retrieving the
    /// songs.
    async fn find_recently_requested(
        &self,
        limit: u64,
    ) -> Result<Vec<entities::songs::Model>, RepositoryError>;

    /// Find the songs that have been played recently.
    ///
    /// # Errors
    ///
    /// Returns a `RepositoryError` if something goes wrong while retrieving the
    /// songs.
    async fn find_recently_played(
        &self,
        pagination: &PaginationParams,
    ) -> Result<Page<entities::songs::Model>, RepositoryError>;
}

generate_dtos!(
    entities::songs::Entity,
    CreateSongDto {
        file_path: String,
        title: String,
        artist: String,
        album: String,
        duration: f64,
        file_hash: String,
        bitrate: i32,
    },
    UpdateSongDto {
        file_path: Option<String>,
        title: Option<String>,
        artist: Option<String>,
        album: Option<String>,
        duration: Option<f64>,
        file_hash: Option<String>,
        bitrate: Option<i32>,
    }
);

enum OrderBy {
    FilePath,
    Title,
    Artist,
    Album,
    Duration,
    FileHash,
    Bitrate,
}

enum OrderDirection {
    Asc,
    Desc,
}

#[derive(Default)]
pub struct SongFilter {
    file_path: Option<String>,
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    duration: Option<f64>,
    file_hash: Option<String>,
    bitrate: Option<i32>,
    page: Option<u64>,
    page_size: Option<u64>,
    search: Option<String>,
    order_by: Option<OrderBy>,
    order_direction: Option<OrderDirection>,
    favourited_by: Option<i64>,
}

impl SongFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn search(mut self, search: &str) -> Self {
        self.search = Some(search.to_string());
        self
    }

    pub fn artist(mut self, artist: &str) -> Self {
        self.artist = Some(artist.to_string());
        self
    }

    pub fn album(mut self, album: &str) -> Self {
        self.album = Some(album.to_string());
        self
    }

    pub fn title(mut self, title: &str) -> Self {
        self.title = Some(title.to_string());
        self
    }

    pub fn duration(mut self, duration: f64) -> Self {
        self.duration = Some(duration);
        self
    }

    pub fn file_hash(mut self, file_hash: &str) -> Self {
        self.file_hash = Some(file_hash.to_string());
        self
    }

    pub fn bitrate(mut self, bitrate: i32) -> Self {
        self.bitrate = Some(bitrate);
        self
    }

    pub fn page(mut self, page: u64) -> Self {
        self.page = Some(page);
        self
    }

    pub fn page_size(mut self, page_size: u64) -> Self {
        self.page_size = Some(page_size);
        self
    }

    pub fn order_by(mut self, order_by: OrderBy) -> Self {
        self.order_by = Some(order_by);
        self
    }

    pub fn order_direction(mut self, order_direction: OrderDirection) -> Self {
        self.order_direction = Some(order_direction);
        self
    }

    pub fn favourited_by(mut self, user_id: i64) -> Self {
        self.favourited_by = Some(user_id);
        self
    }
}

#[async_trait::async_trait]
impl ApplyQueryFilter<entities::songs::Entity> for SongFilter {
    async fn apply(
        &self,
        query: Select<entities::songs::Entity>,
    ) -> Select<entities::songs::Entity> {
        let mut query = query;

        if let Some(search) = &self.search {
            let ts_vec = to_tsvector(search);
            let ts_query = ts_vec
                .lexemes
                .iter()
                .map(|lexeme| format!("{}:*", lexeme.term))
                .collect::<Vec<_>>()
                .join(" & ");
            query = query
                .inner_join(entities::songs_fulltext::Entity)
                .filter(entities::songs_fulltext::Column::Tsvector.full_text_search(ts_query));
        }

        if let Some(favourited_by) = self.favourited_by {
            query = query
                .inner_join(entities::favourite_songs::Entity)
                .filter(entities::favourite_songs::Column::UserId.eq(favourited_by));
        }

        if let Some(file_path) = &self.file_path {
            query = query.filter(entities::songs::Column::FilePath.eq(file_path));
        }

        if let Some(title) = &self.title {
            query = query.filter(entities::songs::Column::Title.eq(title));
        }

        if let Some(artist) = &self.artist {
            query = query.filter(entities::songs::Column::Artist.eq(artist));
        }

        if let Some(album) = &self.album {
            query = query.filter(entities::songs::Column::Album.eq(album));
        }

        if let Some(duration) = self.duration {
            query = query.filter(entities::songs::Column::Duration.eq(duration));
        }

        if let Some(file_hash) = &self.file_hash {
            query = query.filter(entities::songs::Column::FileHash.eq(file_hash));
        }

        if let Some(bitrate) = self.bitrate {
            query = query.filter(entities::songs::Column::Bitrate.eq(bitrate));
        }

        query
    }

    fn page_size(&self) -> u64 {
        self.page_size.unwrap_or(20)
    }

    fn page(&self) -> u64 {
        self.page.unwrap_or(1)
    }
}

#[async_trait::async_trait]
impl SongRepositoryExt for BaseRepository<entities::songs::Entity> {
    async fn delete_by_hash(&self, file_hash: &str) -> Result<(), RepositoryError> {
        entities::songs_fulltext::Entity::delete_by_id(file_hash)
            .exec(&self.db)
            .await
            .map_err(RepositoryError::from)?;
        entities::songs::Entity::delete_many()
            .filter(entities::songs::Column::FileHash.eq(file_hash))
            .exec(&self.db)
            .await?;

        Ok(())
    }

    async fn prune(&self) -> Result<(), RepositoryError> {
        entities::songs_fulltext::Entity::delete_many()
            .exec(&self.db)
            .await?;
        entities::songs::Entity::delete_many()
            .exec(&self.db)
            .await?;

        Ok(())
    }

    async fn find_by_hash(
        &self,
        file_hash: &str,
    ) -> Result<Option<entities::songs::Model>, RepositoryError> {
        entities::songs::Entity::find()
            .filter(entities::songs::Column::FileHash.eq(file_hash))
            .one(&self.db)
            .await
            .map_err(RepositoryError::from)
    }

    async fn count(&self) -> Result<u64, RepositoryError> {
        entities::songs::Entity::find()
            .count(&self.db)
            .await
            .map_err(RepositoryError::from)
    }

    async fn find_recently_requested(
        &self,
        limit: u64,
    ) -> Result<Vec<entities::songs::Model>, RepositoryError> {
        entities::songs::Entity::find()
            .inner_join(entities::song_requests::Entity)
            .order_by_desc(entities::song_requests::Column::CreatedAt)
            .limit(limit)
            .all(&self.db)
            .await
            .map_err(RepositoryError::from)
    }

    async fn find_recently_played(
        &self,
        pagination: &PaginationParams,
    ) -> Result<Page<entities::songs::Model>, RepositoryError> {
        let paginator = entities::songs::Entity::find()
            .inner_join(entities::played_songs::Entity)
            .order_by_desc(entities::played_songs::Column::PlayedAt)
            .paginate(&self.db, pagination.page_size);

        let page = paginator.fetch_page(pagination.page).await?;
        let total = paginator.num_items_and_pages().await?;

        Ok(Page::new(
            page,
            total.number_of_items,
            pagination.page,
            pagination.page_size,
            total.number_of_pages,
        ))
    }
}

mod custom_entity {
    use sea_orm::entity::prelude::*;

    use crate::entities;

    #[derive(Clone, Copy, Debug, EnumIter)]
    pub enum SongRelation {
        SongRequests,
        PlayedSongs,
        FavouriteSongs,
        SongsFulltext,
    }

    #[derive(Clone, Copy, Debug, EnumIter)]
    pub enum SongRequestRelation {
        Song,
    }

    #[derive(Clone, Copy, Debug, EnumIter)]
    pub enum PlayedSongRelation {
        Song,
    }

    #[derive(Clone, Copy, Debug, EnumIter)]
    pub enum FavouriteSongRelation {
        Song,
    }

    #[derive(Clone, Copy, Debug, EnumIter)]
    pub enum SongsFulltextRelation {
        Song,
    }

    impl RelationTrait for SongRelation {
        fn def(&self) -> RelationDef {
            match self {
                SongRelation::SongRequests => {
                    entities::song_requests::Entity::belongs_to(entities::songs::Entity)
                        .from(entities::song_requests::Column::SongId)
                        .to(entities::songs::Column::FileHash)
                        .into()
                }
                SongRelation::PlayedSongs => {
                    entities::played_songs::Entity::belongs_to(entities::songs::Entity)
                        .from(entities::played_songs::Column::SongId)
                        .to(entities::songs::Column::FileHash)
                        .into()
                }
                SongRelation::FavouriteSongs => {
                    entities::favourite_songs::Entity::belongs_to(entities::songs::Entity)
                        .from(entities::favourite_songs::Column::SongId)
                        .to(entities::songs::Column::FileHash)
                        .into()
                }
                SongRelation::SongsFulltext => {
                    entities::songs_fulltext::Entity::belongs_to(entities::songs::Entity)
                        .from(entities::songs_fulltext::Column::SongId)
                        .to(entities::songs::Column::FileHash)
                        .into()
                }
            }
        }
    }

    impl RelationTrait for SongRequestRelation {
        fn def(&self) -> RelationDef {
            match self {
                SongRequestRelation::Song => {
                    entities::songs::Entity::has_many(entities::song_requests::Entity).into()
                }
            }
        }
    }

    impl RelationTrait for PlayedSongRelation {
        fn def(&self) -> RelationDef {
            match self {
                PlayedSongRelation::Song => {
                    entities::songs::Entity::has_many(entities::played_songs::Entity).into()
                }
            }
        }
    }

    impl RelationTrait for FavouriteSongRelation {
        fn def(&self) -> RelationDef {
            match self {
                FavouriteSongRelation::Song => {
                    entities::songs::Entity::has_many(entities::favourite_songs::Entity).into()
                }
            }
        }
    }

    impl RelationTrait for SongsFulltextRelation {
        fn def(&self) -> RelationDef {
            match self {
                SongsFulltextRelation::Song => {
                    entities::songs::Entity::has_many(entities::songs_fulltext::Entity).into()
                }
            }
        }
    }

    impl Related<entities::songs::Entity> for entities::song_requests::Entity {
        fn to() -> RelationDef {
            SongRelation::SongRequests.def()
        }
    }

    impl Related<entities::song_requests::Entity> for entities::songs::Entity {
        fn to() -> RelationDef {
            SongRequestRelation::Song.def()
        }
    }

    impl Related<entities::songs::Entity> for entities::played_songs::Entity {
        fn to() -> RelationDef {
            PlayedSongRelation::Song.def()
        }
    }

    impl Related<entities::played_songs::Entity> for entities::songs::Entity {
        fn to() -> RelationDef {
            PlayedSongRelation::Song.def()
        }
    }

    impl Related<entities::songs::Entity> for entities::favourite_songs::Entity {
        fn to() -> RelationDef {
            FavouriteSongRelation::Song.def()
        }
    }

    impl Related<entities::songs::Entity> for entities::songs_fulltext::Entity {
        fn to() -> RelationDef {
            SongsFulltextRelation::Song.def()
        }
    }

    impl Related<entities::songs_fulltext::Entity> for entities::songs::Entity {
        fn to() -> RelationDef {
            SongRelation::SongsFulltext.def().rev()
        }
    }

    impl Related<entities::favourite_songs::Entity> for entities::songs::Entity {
        fn to() -> RelationDef {
            FavouriteSongRelation::Song.def().rev()
        }
    }
}
