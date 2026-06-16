CREATE TABLE news_digest (
    digest_date TEXT PRIMARY KEY,
    created_at  TEXT NOT NULL
);

CREATE TABLE news_article (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    digest_date TEXT NOT NULL REFERENCES news_digest(digest_date) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    title       TEXT NOT NULL,
    url         TEXT NOT NULL,
    source      TEXT NOT NULL,
    score       INTEGER NOT NULL DEFAULT 0,
    summary     TEXT NOT NULL,
    key_points  TEXT NOT NULL
);

CREATE TABLE news_quiz_question (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    digest_date  TEXT NOT NULL REFERENCES news_digest(digest_date) ON DELETE CASCADE,
    position     INTEGER NOT NULL,
    article_pos  INTEGER,
    question     TEXT NOT NULL,
    options      TEXT NOT NULL,
    answer_index INTEGER NOT NULL,
    explanation  TEXT
);

CREATE TABLE news_seen (
    url_hash   TEXT PRIMARY KEY,
    url        TEXT NOT NULL,
    first_seen TEXT NOT NULL
);
