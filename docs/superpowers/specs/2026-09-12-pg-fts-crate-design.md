# `pg_fts` — Postgres full-text search for SeaORM

Date: 2026-09-12
Status: approved design, not yet implemented

## Problem

`src/vectorizer.rs` reimplements Postgres's `english` text search
configuration in Rust (`rust-stemmers` + `stopwords`). Its only consumer is
search-query construction: it stems the user's input, then joins the stems into
a `to_tsquery` string of the form `a:* & b:*`
(`src/repositories/songs.rs:253`, `src/dtos/songs.rs:169`).

The indexed side is genuine Postgres: `songs_fulltext.tsvector` is a
`GENERATED ALWAYS AS (to_tsvector('english', title) || to_tsvector('english',
artist) || to_tsvector('english', album)) STORED` column.

The two sides use different stemmers. Snowball-via-`rust-stemmers` and the
`stopwords` crate's Spark list do not agree with Postgres's `english`
dictionary chain on stopwords, numeric and unicode tokenization, or several
irregular stems. Every disagreement is a query that silently returns no rows
for input that is present in the index. The mismatch cannot be fixed by
improving the Rust vectorizer; only Postgres knows what Postgres indexed.

Three further defects fall out of the same design:

1. The tsquery string is interpolated from user input. It is passed as a bound
   parameter, so it is not SQL injection, but a `to_tsquery` syntax error is a
   500 rather than an empty result set.
2. There is no GIN index on `songs_fulltext.tsvector`. Every search is a
   sequential scan with a `@@` evaluation per row.
3. Results cannot be ordered by relevance. `OrderBy` (`src/dtos/songs.rs:113`)
   offers only Title/Artist/Album/Duration/Bitrate.

## What sea-query already provides

SeaORM 2.0 rides on sea-query 1.0, which already ships, in
`sea_query::extension::postgres`:

- `PgFunc::to_tsquery`, `plainto_tsquery`, `phraseto_tsquery`,
  `websearch_to_tsquery`
- `PgFunc::ts_rank`, `ts_rank_cd`
- `ExprTrait::matches()` — the `@@` operator (`PgBinOper::Matches`)

`src/pg_extension.rs` duplicates the last two of these and should be deleted.

What sea-query does **not** provide, and what this crate exists to add:

- **regconfig by name.** Every `PgFunc` tsquery constructor takes
  `regconfig: Option<u32>` — a raw catalog OID. There is no way to say
  `'english'` without dropping to `Expr::cust`.
- **A tsquery as a reusable value.** `@@` and `ts_rank` must be given the
  *same* tsquery or ranking silently disagrees with matching. sea-query offers
  no type that carries one query expression to both call sites.
- **Prefix (type-ahead) queries** that are still parsed by Postgres.
- **Schema support.** No `tsvector` column type, no generated-column builder,
  no GIN/GiST index helper. Fulltext DDL in this repo is raw
  `execute_unprepared` strings.

## Scope

In scope, per the brainstorming session:

- Matching (`@@`) and relevance ranking (`ts_rank` / `ts_rank_cd`).
- Migration/schema helpers: `tsvector` column type, generated-column builder,
  GIN index helper.
- Query semantics: **websearch with a prefix on the final lexeme.**

Out of scope (explicitly deferred, not forgotten):

- `ts_headline` highlighting. Nothing in caliborn renders snippets today.
- `setweight` / A-B-C-D weighted columns. The crate must not *prevent* them —
  the generated-column builder takes a list of source expressions, so a weight
  wrapper can be added later without an API break — but no weight API ships in
  v1.
- Publishing to crates.io. The crate is a workspace member named `pg_fts`.
  If it proves out, publishing is a separate decision.

## Query semantics

A user's search string means: *websearch syntax, with the word currently being
typed treated as a prefix.*

The generated expression is:

```sql
CASE
  WHEN WEBSEARCH_TO_TSQUERY('english'::regconfig, $1)::text = ''
    THEN WEBSEARCH_TO_TSQUERY('english'::regconfig, $1)
  ELSE TO_TSQUERY(
    'english'::regconfig,
    WEBSEARCH_TO_TSQUERY('english'::regconfig, $1)::text || ':*'
  )
END
```

Rationale for each part:

- `websearch_to_tsquery` never raises a syntax error on arbitrary user input —
  it is the only tsquery constructor Postgres guarantees this for. It also
  gives quoted phrases, `or`, and `-negation` for free.
- Rendering it back to `::text` and re-parsing with `to_tsquery` is what
  allows `:*` to be attached to the last lexeme. The intermediate text is
  Postgres's own normalized output (`'lumi' & 'radio'`), never the raw user
  string, so this re-parse cannot be driven by user input.
- The `CASE` guard covers the input that reduces to an empty tsquery — an
  empty string, or a string of nothing but stopwords such as `"the"`. Without
  it the concatenation yields `':*'`, which `to_tsquery` rejects at runtime.
  In the empty branch the match simply returns no rows, which is the correct
  answer.

The user input appears exactly once as a bound parameter, repeated by the
expression builder; it is never concatenated into SQL text.

## Crate design: expression-first

`pg_fts` exposes constructors that return sea-query expressions and knows
nothing about caliborn's entities. No proc macros. No wrapping of SeaORM's
`Select`.

```
pg_fts/
  Cargo.toml
  src/lib.rs        # re-exports, crate docs
  src/config.rs     # TsConfig
  src/query.rs      # TsQuery
  src/expr.rs       # FtsExprTrait
  src/schema.rs     # migration helpers
  tests/sql.rs      # generated-SQL assertions
```

Dependencies: `sea-query` 1.0 with `backend-postgres` and `derive`, and
`thiserror`. It does **not** depend on `sea-orm`; column arguments are taken as
`impl IntoColumnRef`, which sea-query blanket-implements for anything
`IntoIden` (so SeaORM's `Column` enums qualify) and for `(table, column)`
tuples (needed here, because the songs search joins two tables and the
reference must be qualified).

### `TsConfig`

```rust
pub struct TsConfig(Cow<'static, str>);

impl TsConfig {
    pub const ENGLISH: TsConfig;
    pub const SIMPLE: TsConfig;
    pub fn new(name: &str) -> Result<TsConfig, FtsError>;
}
```

`new` validates against `^[a-z_][a-z0-9_]*$` and rejects anything else, because
the value is rendered as a literal (`'english'::regconfig`) rather than bound —
sea-query's OID-only parameter leaves no bound-parameter path. The consts are
infallible. Validation is what keeps a config name from becoming an injection
vector if one ever arrives from configuration rather than a literal.

### `TsQuery`

The reusable value. Built once, passed to both the filter and the ranking, so
they cannot drift apart.

```rust
#[derive(Clone)]
pub struct TsQuery { /* config + mode + input */ }

impl TsQuery {
    pub fn websearch_prefix(config: TsConfig, input: impl Into<String>) -> Self;
    pub fn websearch(config: TsConfig, input: impl Into<String>) -> Self;
    pub fn plain(config: TsConfig, input: impl Into<String>) -> Self;
    pub fn phrase(config: TsConfig, input: impl Into<String>) -> Self;

    pub fn rank(&self, vector: impl IntoColumnRef) -> SimpleExpr;
    pub fn rank_cd(&self, vector: impl IntoColumnRef) -> SimpleExpr;
}

impl From<&TsQuery> for SimpleExpr { /* the CASE expression above */ }
```

The three non-prefix modes are thin wrappers over sea-query's existing
constructors; they exist so that `TsConfig` is usable at all (sea-query's own
constructors demand an OID) and so the mode is a property of the value rather
than a call-site decision. They are not new functionality.

### `FtsExprTrait`

```rust
pub trait FtsExprTrait {
    fn fts_matches(self, query: &TsQuery) -> SimpleExpr;  // @@
}

impl<T: IntoColumnRef> FtsExprTrait for T {}
```

Delegates to sea-query's `PgBinOper::Matches`. It exists to take a `&TsQuery`
rather than a bare expression, which is what makes "same query for filter and
rank" the path of least resistance.

### Schema helpers

```rust
pub fn tsvector() -> ColumnType;                    // custom("tsvector")

pub struct FtsColumn { /* ... */ }
impl FtsColumn {
    pub fn new(name: impl IntoIden, config: TsConfig) -> Self;
    pub fn source(self, column: impl IntoIden) -> Self;   // repeatable
    pub fn to_column_def(&self) -> ColumnDef;             // GENERATED ALWAYS AS (...) STORED
}

pub fn gin_index(table: impl IntoIden, column: impl IntoIden) -> IndexCreateStatement;
```

`FtsColumn` emits
`to_tsvector('english', coalesce(col, '')) || …` over its sources. The
`coalesce` is deliberate and is a behavioural fix: in Postgres, a NULL operand
makes the whole concatenated tsvector NULL, so one NULL source column erases
the row from the index entirely. The existing `songs_fulltext` DDL omits it and
is only safe because all three source columns are `NOT NULL`.

`gin_index` emits `CREATE INDEX ... USING GIN (col)`. sea-query renders
`IndexType::FullText` as `GIN` on Postgres, so this is a convenience wrapper
that names the intent.

## Integration into caliborn

1. **New crate** `pg_fts/` added to `[workspace] members`.
2. **Delete** `src/vectorizer.rs` and `src/pg_extension.rs`; drop the
   `rust-stemmers` and `stopwords` dependencies from `Cargo.toml`.
3. **Delete** `SearchParams::as_ts_query` (`src/dtos/songs.rs:167`). Its only
   caller is `examples/playground.rs:17`, which is updated to build a
   `TsQuery`.
4. **`SongFilter::apply`** (`src/repositories/songs.rs:251`) builds one
   `TsQuery::websearch_prefix(TsConfig::ENGLISH, search)` and uses it for the
   `@@` filter; when `order_by` is `Relevance` it also supplies the
   `ORDER BY ts_rank(...) DESC`.
5. **`OrderBy`** gains a `Relevance` variant in both
   `src/dtos/songs.rs` and `src/repositories/songs.rs`. `Relevance` without a
   `search` term has no meaning; it falls back to `Title` ascending rather than
   erroring, and this is asserted in a test. Relevance is *not* made the
   default order for searches — that would change existing API responses.
   Clients opt in with `order[by]=relevance`.
6. **New migration** `m20260912_*_add_songs_fulltext_gin_index`, additive,
   creating the missing GIN index on `songs_fulltext.tsvector`.
   `CREATE INDEX CONCURRENTLY` is *not* used: SeaORM migrations run inside a
   transaction, and `CONCURRENTLY` is illegal there. The table is small
   (one row per song) so a brief lock is acceptable.

No change to the `songs_fulltext` table definition itself, and no data
migration. Per the project's constraint, migrations stay additive.

## Testing

**`pg_fts` unit tests (`tests/sql.rs`)** — assert the exact generated SQL for
each `TsQuery` mode, for `fts_matches`, for `rank`, and for `FtsColumn`'s
generated-column DDL. These are string comparisons against
`Query::select().to_string(PostgresQueryBuilder)`; they pin the shape of the
`CASE` expression and the parameter placeholder count. `TsConfig::new`
rejection cases are covered here too.

**caliborn integration tests** — the repo already has a real-Postgres harness
(`testcontainers-modules/postgres`, `tests/common.rs`) and fixtures for both
`songs` and `songs_fulltext`. The tests that matter run against real Postgres,
because the entire point is agreement with Postgres's own stemmer:

- A term whose Rust and Postgres stems differ matches correctly. This is the
  regression test for the bug being fixed; the fixture must contain at least
  one such song, identified by querying Postgres for a stem that
  `rust-stemmers` disagrees with before the fixture is written.
- Prefix search: a partial word matches a longer indexed word.
- Input that is entirely stopwords returns an empty result, not a 500.
- Input containing tsquery metacharacters (`&`, `|`, `!`, `:`, `(`, unbalanced
  quotes) returns a result set, not a 500. This is the direct regression test
  for the `to_tsquery` syntax-error defect.
- `order[by]=relevance` orders by rank; a song matching in the title outranks
  one matching only in the album.
- `order[by]=relevance` with no search term falls back to title order.
- An `EXPLAIN` assertion that the search query uses the GIN index rather than a
  sequential scan.

Existing song search tests in `tests/songs.rs` must pass unchanged, except
where they assert behaviour the Rust stemmer got wrong.

## Risks

- **Result-set changes.** Moving to Postgres's stemmer changes which rows match
  for some queries. That is the intended fix, but it is a user-visible change
  to an existing endpoint. The integration tests should capture a before/after
  for a handful of representative queries so the change is documented rather
  than discovered.
- **`websearch_to_tsquery` reserves `or` and `-`.** A user searching for a song
  titled `"- or -"` now gets websearch operator semantics. Acceptable; the
  alternative is worse.
- **The `::text` round-trip is unusual.** It is the standard technique for
  prefix-enabled websearch queries, but it deserves a comment in the source
  pointing at this document, or the next reader will try to "simplify" it.
