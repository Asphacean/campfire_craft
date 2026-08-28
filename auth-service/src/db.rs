//! SQLite access. One `rusqlite::Connection` behind a `Mutex` — no pool, no
//! async SQLite wrapper: there are seven users and one writer.
//!
//! Every statement in this file binds its inputs as SQL parameters. No
//! user-supplied value is ever spliced into a query string — this is the
//! Tampering mitigation the threat register (T-02-01-03) gates on, and the
//! gate is a repo-wide grep for a string-built SQL call.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

pub struct Db {
    conn: Mutex<Connection>,
}

/// A registered account row, keyed by the case-insensitive `nick_lower`
/// index but carrying the *original* registration casing — RESEARCH.md
/// Pitfall 5: the game derives the offline UUID from the exact byte string,
/// so a case mismatch silently orphans a player's inventory. `/validate`
/// always echoes `nick`, never `nick_lower`.
pub struct User {
    pub id: i64,
    pub nick: String,
    pub pw_hash: String,
}

/// Outcome of an insert attempt that can legitimately collide on the unique
/// index — the caller maps this to 409, not a 500.
pub enum InsertUserResult {
    Created,
    NickTaken,
}

/// A candidate unexpired, unconsumed token row for a given user. `/validate`
/// argon2-verifies the presented token against each candidate in turn.
pub struct TokenCandidate {
    pub id: i64,
    pub token_hash: String,
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before 1970")
        .as_secs() as i64
}

impl Db {
    /// Open (creating if absent) the accounts database at `path` and ensure
    /// the schema exists.
    pub fn open(path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS users (
                id         INTEGER PRIMARY KEY,
                nick       TEXT NOT NULL,
                nick_lower TEXT NOT NULL UNIQUE,
                pw_hash    TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS tokens (
                id          INTEGER PRIMARY KEY,
                user_id     INTEGER NOT NULL REFERENCES users(id),
                token_hash  TEXT NOT NULL,
                expires_at  INTEGER NOT NULL,
                consumed_at INTEGER,
                created_at  INTEGER NOT NULL
            );",
        )?;

        // D-13: the database file must end up mode 600 — SQLite's own
        // default (governed by the process umask) is not assumed here.
        // WAL mode (set above) creates `-wal`/`-shm` sibling files
        // alongside the main file; every one of the three that exists at
        // this point gets the same treatment.
        for suffix in ["", "-wal", "-shm"] {
            let sibling = format!("{path}{suffix}");
            if std::path::Path::new(&sibling).exists() {
                let _ = std::fs::set_permissions(
                    &sibling,
                    std::os::unix::fs::PermissionsExt::from_mode(0o600),
                );
            }
        }

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Insert a new account. `nick` keeps the exact registration casing;
    /// `nick_lower` (computed by the caller) is the case-insensitive
    /// uniqueness key (D-04).
    pub fn insert_user(
        &self,
        nick: &str,
        nick_lower: &str,
        pw_hash: &str,
    ) -> rusqlite::Result<InsertUserResult> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let result = conn.execute(
            "INSERT INTO users (nick, nick_lower, pw_hash, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![nick, nick_lower, pw_hash, now_unix()],
        );
        match result {
            Ok(_) => Ok(InsertUserResult::Created),
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Ok(InsertUserResult::NickTaken)
            }
            Err(e) => Err(e),
        }
    }

    /// Look a user up by the lowercased nick (case-insensitive per D-04).
    pub fn find_user_by_nick_lower(&self, nick_lower: &str) -> rusqlite::Result<Option<User>> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        conn.query_row(
            "SELECT id, nick, pw_hash FROM users WHERE nick_lower = ?1",
            params![nick_lower],
            |row| {
                Ok(User {
                    id: row.get(0)?,
                    nick: row.get(1)?,
                    pw_hash: row.get(2)?,
                })
            },
        )
        .optional()
    }

    /// Overwrite a user's stored password hash (operator `reset`, D-05).
    /// Returns `false` if no such nick exists.
    pub fn update_pw_hash(&self, nick_lower: &str, pw_hash: &str) -> rusqlite::Result<bool> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let changed = conn.execute(
            "UPDATE users SET pw_hash = ?1 WHERE nick_lower = ?2",
            params![pw_hash, nick_lower],
        )?;
        Ok(changed == 1)
    }

    /// Store a newly issued token's hash for `user_id`, expiring at
    /// `expires_at` (unix seconds).
    pub fn insert_token(
        &self,
        user_id: i64,
        token_hash: &str,
        expires_at: i64,
    ) -> rusqlite::Result<()> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        conn.execute(
            "INSERT INTO tokens (user_id, token_hash, expires_at, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![user_id, token_hash, expires_at, now_unix()],
        )?;
        Ok(())
    }

    /// All unexpired, unconsumed token candidates for `user_id`, newest
    /// first (an argon2-verify loop stops at the first hash match).
    pub fn candidate_tokens(&self, user_id: i64, now: i64) -> rusqlite::Result<Vec<TokenCandidate>> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, token_hash FROM tokens
             WHERE user_id = ?1 AND consumed_at IS NULL AND expires_at > ?2
             ORDER BY id DESC",
        )?;
        let rows = stmt
            .query_map(params![user_id, now], |row| {
                Ok(TokenCandidate {
                    id: row.get(0)?,
                    token_hash: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Atomically consume a token: the `consumed_at IS NULL` predicate makes
    /// this a compare-and-swap, not a check-then-set — the security property
    /// T-02-01-04 depends on. Returns `true` only if exactly one row changed
    /// (i.e. this call won the race to consume it).
    pub fn consume_token(&self, token_id: i64, now: i64) -> rusqlite::Result<bool> {
        let conn = self.conn.lock().expect("db mutex poisoned");
        let changed = conn.execute(
            "UPDATE tokens SET consumed_at = ?1 WHERE id = ?2 AND consumed_at IS NULL",
            params![now, token_id],
        )?;
        Ok(changed == 1)
    }
}
