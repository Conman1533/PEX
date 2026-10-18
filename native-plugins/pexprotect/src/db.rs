//! The SQLite block log. Plain rusqlite; no server types.

use rusqlite::{Connection, params_from_iter, types::Value};
use std::path::Path;

/// One logged block change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub id: i64,
    pub time: i64,
    pub world: String,
    pub pos: [i32; 3],
    /// Player name, or `#fire`, `#tnt`, `#creeper` and similar for non-player causes.
    pub actor: String,
    pub action: String,
    pub old_state: u16,
    pub new_state: u16,
    pub rolled_back: bool,
}

/// What a lookup, rollback or restore applies to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub actors: Vec<String>,
    pub since: Option<i64>,
    pub action: Option<String>,
    pub world: Option<String>,
    /// Center and radius; `None` means everywhere.
    pub area: Option<([i32; 3], i32)>,
    /// Only records with this rolled-back state, if set.
    pub rolled_back: Option<bool>,
}

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    // WAL lets command lookups read while the writer thread commits.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    init(&conn)?;
    Ok(conn)
}

pub fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS blocks (
            id INTEGER PRIMARY KEY,
            time INTEGER NOT NULL,
            world TEXT NOT NULL,
            x INTEGER NOT NULL,
            y INTEGER NOT NULL,
            z INTEGER NOT NULL,
            actor TEXT NOT NULL,
            action TEXT NOT NULL,
            old_state INTEGER NOT NULL,
            new_state INTEGER NOT NULL,
            rolled_back INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS blocks_pos ON blocks (world, x, z, y);
        CREATE INDEX IF NOT EXISTS blocks_time ON blocks (time);
        CREATE INDEX IF NOT EXISTS blocks_actor ON blocks (actor, time);",
    )
}

pub fn insert(conn: &mut Connection, records: &[Record]) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO blocks (time, world, x, y, z, actor, action, old_state, new_state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;
        for r in records {
            stmt.execute(rusqlite::params![
                r.time,
                r.world,
                r.pos[0],
                r.pos[1],
                r.pos[2],
                r.actor,
                r.action,
                r.old_state,
                r.new_state
            ])?;
        }
    }
    tx.commit()
}

pub fn set_rolled_back(
    conn: &mut Connection,
    ids: &[i64],
    rolled_back: bool,
) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare_cached("UPDATE blocks SET rolled_back = ?1 WHERE id = ?2")?;
        for id in ids {
            stmt.execute(rusqlite::params![rolled_back, id])?;
        }
    }
    tx.commit()
}

/// Deletes records older than `before`, returning how many went.
pub fn purge(conn: &Connection, before: i64) -> rusqlite::Result<usize> {
    conn.execute("DELETE FROM blocks WHERE time < ?1", [before])
}

/// Matching records, newest first unless `oldest_first`, at most `limit`.
pub fn query(
    conn: &Connection,
    filter: &Filter,
    oldest_first: bool,
    limit: usize,
) -> rusqlite::Result<Vec<Record>> {
    let mut sql = String::from(
        "SELECT id, time, world, x, y, z, actor, action, old_state, new_state, rolled_back
         FROM blocks WHERE 1 = 1",
    );
    let mut args: Vec<Value> = Vec::new();
    if !filter.actors.is_empty() {
        sql.push_str(" AND actor IN (");
        sql.push_str(&vec!["?"; filter.actors.len()].join(", "));
        sql.push(')');
        args.extend(filter.actors.iter().cloned().map(Value::Text));
    }
    if let Some(since) = filter.since {
        sql.push_str(" AND time >= ?");
        args.push(Value::Integer(since));
    }
    if let Some(action) = &filter.action {
        sql.push_str(" AND action = ?");
        args.push(Value::Text(action.clone()));
    }
    if let Some(world) = &filter.world {
        sql.push_str(" AND world = ?");
        args.push(Value::Text(world.clone()));
    }
    if let Some((center, radius)) = filter.area {
        sql.push_str(" AND x BETWEEN ? AND ? AND y BETWEEN ? AND ? AND z BETWEEN ? AND ?");
        for c in center {
            args.push(Value::Integer(i64::from(c) - i64::from(radius)));
            args.push(Value::Integer(i64::from(c) + i64::from(radius)));
        }
    }
    if let Some(rolled_back) = filter.rolled_back {
        sql.push_str(" AND rolled_back = ?");
        args.push(Value::Integer(i64::from(rolled_back)));
    }
    sql.push_str(if oldest_first {
        " ORDER BY time ASC, id ASC"
    } else {
        " ORDER BY time DESC, id DESC"
    });
    sql.push_str(" LIMIT ?");
    args.push(Value::Integer(i64::try_from(limit).unwrap_or(i64::MAX)));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(args), |row| {
        Ok(Record {
            id: row.get(0)?,
            time: row.get(1)?,
            world: row.get(2)?,
            pos: [row.get(3)?, row.get(4)?, row.get(5)?],
            actor: row.get(6)?,
            action: row.get(7)?,
            old_state: row.get(8)?,
            new_state: row.get(9)?,
            rolled_back: row.get(10)?,
        })
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(time: i64, actor: &str, pos: [i32; 3], action: &str) -> Record {
        Record {
            id: 0,
            time,
            world: "world".into(),
            pos,
            actor: actor.into(),
            action: action.into(),
            old_state: 1,
            new_state: 0,
            rolled_back: false,
        }
    }

    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        insert(
            &mut conn,
            &[
                record(100, "Alice", [0, 64, 0], "break"),
                record(200, "Bob", [5, 64, 5], "place"),
                record(300, "Alice", [50, 64, 50], "break"),
                record(400, "#tnt", [1, 64, 1], "explode"),
            ],
        )
        .unwrap();
        conn
    }

    #[test]
    fn filters_by_actor_time_action_and_area() {
        let conn = db();
        let by_alice = Filter {
            actors: vec!["Alice".into()],
            ..Filter::default()
        };
        assert_eq!(query(&conn, &by_alice, false, 10).unwrap().len(), 2);

        let recent = Filter {
            since: Some(250),
            ..Filter::default()
        };
        assert_eq!(query(&conn, &recent, false, 10).unwrap().len(), 2);

        let near_origin = Filter {
            area: Some(([0, 64, 0], 5)),
            ..Filter::default()
        };
        let records = query(&conn, &near_origin, false, 10).unwrap();
        assert_eq!(records.len(), 3);
        // Newest first.
        assert_eq!(records[0].actor, "#tnt");

        let breaks_near_origin = Filter {
            action: Some("break".into()),
            area: Some(([0, 64, 0], 5)),
            ..Filter::default()
        };
        assert_eq!(
            query(&conn, &breaks_near_origin, true, 10).unwrap().len(),
            1
        );
    }

    #[test]
    fn rolled_back_flag_round_trips() {
        let mut conn = db();
        let all = query(&conn, &Filter::default(), true, 10).unwrap();
        set_rolled_back(&mut conn, &[all[0].id, all[1].id], true).unwrap();
        let pending = Filter {
            rolled_back: Some(false),
            ..Filter::default()
        };
        assert_eq!(query(&conn, &pending, true, 10).unwrap().len(), 2);
    }

    #[test]
    fn purge_removes_old_records() {
        let conn = db();
        assert_eq!(purge(&conn, 250).unwrap(), 2);
        assert_eq!(query(&conn, &Filter::default(), true, 10).unwrap().len(), 2);
    }
}
