PRAGMA application_id = 0x414D584E;
PRAGMA user_version = 1;

CREATE TABLE source_snapshots (
    digest TEXT PRIMARY KEY,
    saved INTEGER NOT NULL CHECK (saved IN (0, 1))
) STRICT;

CREATE TABLE paragraph_bindings (
    snapshot TEXT NOT NULL REFERENCES source_snapshots(digest) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL,
    scene TEXT,
    selector TEXT NOT NULL,
    text TEXT NOT NULL,
    paragraph TEXT NOT NULL,
    PRIMARY KEY (snapshot, ordinal)
) STRICT;

CREATE TABLE takes (
    id TEXT PRIMARY KEY,
    paragraph TEXT NOT NULL,
    scene TEXT,
    text TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('recorded', 'imported', 'synthesized')),
    status TEXT NOT NULL CHECK (status IN ('recording', 'complete', 'interrupted')),
    created_ms INTEGER NOT NULL,
    issue TEXT
) STRICT;

CREATE TABLE audio_streams (
    take_id TEXT NOT NULL REFERENCES takes(id) ON DELETE CASCADE,
    role INTEGER NOT NULL CHECK (role IN (0, 1)),
    rate INTEGER NOT NULL CHECK (rate > 0),
    channels INTEGER NOT NULL CHECK (channels > 0),
    frames INTEGER NOT NULL DEFAULT 0 CHECK (frames >= 0),
    PRIMARY KEY (take_id, role)
) STRICT;

CREATE TABLE audio_chunks (
    take_id TEXT NOT NULL,
    role INTEGER NOT NULL,
    start_frame INTEGER NOT NULL,
    pcm BLOB NOT NULL,
    PRIMARY KEY (take_id, role, start_frame),
    FOREIGN KEY (take_id, role) REFERENCES audio_streams(take_id, role) ON DELETE CASCADE
) STRICT;

CREATE TABLE alignments (
    take_id TEXT PRIMARY KEY REFERENCES takes(id) ON DELETE CASCADE,
    transcript BLOB NOT NULL,
    receipt BLOB NOT NULL
) STRICT;

CREATE TABLE mixes (
    audio TEXT NOT NULL,
    transcript TEXT NOT NULL,
    audio_digest TEXT NOT NULL,
    transcript_digest TEXT NOT NULL,
    layout BLOB NOT NULL,
    PRIMARY KEY (audio, transcript)
) STRICT;

CREATE TABLE mix_parts (
    audio TEXT NOT NULL,
    transcript TEXT NOT NULL,
    selector TEXT NOT NULL,
    take_id TEXT NOT NULL REFERENCES takes(id),
    PRIMARY KEY (audio, transcript, selector),
    FOREIGN KEY (audio, transcript) REFERENCES mixes(audio, transcript) ON DELETE CASCADE
) STRICT;
