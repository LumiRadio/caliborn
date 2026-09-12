# Searching the song library

Two endpoints search the library. They take exactly the same parameters and
return exactly the same shape:

| Endpoint | Auth |
|---|---|
| `GET /songs/search` | none |
| `GET /songs/favourites` | user JWT or API key, `use_bot` permission |

Searching is done by Postgres full-text search over each song's **title,
artist and album**. Nothing else is searched - not the file path, not the
duration.

## The `query` parameter

`query` accepts web-search syntax, the same conventions a search engine box
uses.

| You type | It means |
|---|---|
| `back black` | both words must appear |
| `"back in black"` | the words must appear together, in that order |
| `rock or jazz` | either word |
| `rock -pop` | `rock`, but not `pop` |

Everything else is ordinary text. Punctuation is **not** operator syntax: `&`,
`|`, `!`, `:` and brackets are treated as characters in your search terms, not
as instructions. You cannot break the search by typing them.

### The last word is a prefix

The word you are still typing is matched as a prefix, so results narrow as you
type. `unre` finds "Totally **Unre**lated Song"; you do not have to finish the
word.

This applies to the last term only. In `back bla`, `back` must match in full
and `bla` matches as a prefix.

### Words are matched by stem, not by spelling

`testing`, `tested` and `tests` all search for the same underlying word, so
searching `testing` finds a song titled "The Great Test". This is why
searching `test` finds "Testing Hits" even though no song contains the exact
word "test".

### Very common words are ignored

Words like `the`, `a`, `in`, `of` carry no search value and are dropped. A
search for **only** such words - `the` - matches nothing at all, because
there is nothing left to search for.

### Punctuated names are single terms

Postgres treats a punctuated name as one indivisible term, not as its pieces:

| Name | Searchable as |
|---|---|
| `AC/DC` | `AC/DC`, or a prefix of it (`AC/`, `ac/d`) |
| `R.E.M.` | `R.E.M.`, or a prefix |
| `24/7` | `24/7`, or a prefix |
| `user@example.com` | the whole address, or a prefix |

So searching `AC/DC` finds AC/DC - but searching `DC` on its own does **not**,
because `DC` is not a term in the index; it is the tail of one. Search from
the start of the name.

Hyphenated names are the exception: `hip-hop` is indexed as `hip-hop`, `hip`
*and* `hop`, so any of the three finds it.

### Gotcha: an empty `query` finds nothing

`?query=` - the parameter present but empty - is a search for nothing, and
matches no songs. To list the whole library, **omit** `query` entirely.

## Filters

Filters are separate from `query` and combine with it, and with each other, by
AND. Unlike `query`, the three text filters are plain case-insensitive
substring matches - no stemming, no prefix logic, no web-search syntax.

| Parameter | Matches |
|---|---|
| `filter[title]` | title contains this text |
| `filter[artist]` | artist contains this text |
| `filter[album]` | album contains this text |
| `filter[duration_min]` / `filter[duration_max]` | duration in seconds, inclusive |
| `filter[bitrate_min]` / `filter[bitrate_max]` | bitrate in bits per second, inclusive |
| `filter[favourited_by]` | only songs favourited by this user id |

Because `filter[artist]` is a substring match, `filter[artist]=DC` **does**
find AC/DC where `query=DC` does not. Use the filters when you know the exact
text; use `query` when you want search.

## Ordering

`order[by]` accepts `relevance`, `title`, `artist`, `album`, `duration`,
`bitrate`. `order[direction]` accepts `asc` or `desc`.

- Default ordering is `title` ascending.
- `relevance` ranks by how well each song matches `query` - how often the
  search terms appear across its title, artist and album. It defaults to
  **descending** (best match first); every other ordering defaults to
  ascending. Passing `order[direction]=asc` with `relevance` is honoured, and
  gives you the worst matches first.
- `relevance` without a `query` has nothing to rank, and falls back to title
  ascending rather than failing.

## Pagination

| Parameter | Default |
|---|---|
| `page` | `0` - pages are zero-based |
| `page_size` | `10` |

The response wraps the results:

```json
{
  "items": [ /* songs */ ],
  "total": 42,
  "page": 0,
  "page_size": 10,
  "total_pages": 5
}
```

## For maintainers

The matching and ranking expressions are built by the `pg_fts` crate in this
workspace; `songs_fulltext.tsvector` is a generated column maintained by
Postgres.
