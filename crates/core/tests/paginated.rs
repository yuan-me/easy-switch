use easy_switch_core::{Settings, history, journal::Journal, sessions};
use serde_json::json;
use std::{fs, path::PathBuf};
use tokio_util::sync::CancellationToken;
fn fixture() -> (tempfile::TempDir, Settings, String, PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let dir = home.join("sessions/2026/09/29");
    fs::create_dir_all(&dir).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let revision = uuid::Uuid::new_v4();
    let a = dir.join(format!("rollout-2026-09-29T00-00-00-{id}.jsonl"));
    let b = dir.join(format!("rollout-2026-09-29T00-00-01-{id}_{revision}.jsonl"));
    let line = |text: &str| {
        format!(
            "{}\n",
            json!({"type":"event_msg","payload":{"type":"user_message","message":text}})
        )
    };
    let meta = |base: serde_json::Value| {
        format!(
            "{}{}\n",
            json!({"type":"session_meta","payload":{"id":id,"session_id":id,"history_mode":"paginated","history_base":base,"cwd":"C:/project","model_provider":"p_123456789012","unknown":17}}),
            " ".repeat(60)
        )
    };
    fs::write(
        &a,
        format!("{}{}", meta(serde_json::Value::Null), line("inherited")),
    )
    .unwrap();
    let boundary = fs::metadata(&a).unwrap().len();
    fs::write(
        &b,
        format!(
            "{}{}",
            meta(json!({"thread_id":id,"end_byte_offset":boundary,"end_ordinal_exclusive":2})),
            line("current")
        ),
    )
    .unwrap();
    let db = rusqlite::Connection::open(home.join("state.sqlite")).unwrap();
    db.execute_batch("CREATE TABLE threads(id TEXT PRIMARY KEY,title TEXT,cwd TEXT,model_provider TEXT,rollout_path TEXT,updated_at INTEGER,archived INTEGER)").unwrap();
    db.execute(
        "INSERT INTO threads VALUES(?1,'test','C:/project','p_123456789012',?2,1,0)",
        rusqlite::params![id, b.to_string_lossy()],
    )
    .unwrap();
    (
        tmp,
        Settings {
            codex_home: home,
            ..Default::default()
        },
        id,
        a,
        b,
    )
}
#[test]
fn reverted_thread_is_one_session_and_reads_inherited_prefix() {
    let (_t, s, _, a, b) = fixture();
    assert!(history::same_paginated_thread(&a, &b).unwrap());
    let scan = sessions::Scanner::default()
        .scan(&s, &CancellationToken::new())
        .unwrap();
    assert!(scan.warnings.is_empty());
    assert_eq!(scan.sessions.len(), 1);
    assert_eq!(scan.sessions[0].related_paths, vec![a]);
    let d = sessions::detail(&scan.sessions[0], 100000).unwrap();
    assert_eq!(
        d.messages
            .iter()
            .map(|m| m.text.as_str())
            .collect::<Vec<_>>(),
        vec!["inherited", "current"]
    );
}
#[test]
fn paginated_repair_preserves_offsets_and_all_nonmetadata_bytes() {
    let (t, s, _, a, b) = fixture();
    let rows = sessions::Scanner::default()
        .scan(&s, &CancellationToken::new())
        .unwrap()
        .sessions;
    let before = [fs::read(&a).unwrap(), fs::read(&b).unwrap()];
    let mut j = Journal::new(&t.path().join("store"), "repair").unwrap();
    sessions::stage_changes(
        &mut j,
        &s,
        &rows,
        "provider",
        Some("openai"),
        &CancellationToken::new(),
    )
    .unwrap();
    j.commit().unwrap();
    for (p, old) in [&a, &b].iter().zip(before) {
        let new = fs::read(p).unwrap();
        let offset = old.iter().position(|b| *b == b'\n').unwrap() + 1;
        assert_eq!(new.len(), old.len());
        assert_eq!(new[offset..], old[offset..]);
    }
    assert_eq!(
        sessions::detail(&rows[0], 100000).unwrap().messages.len(),
        2
    );
}
#[test]
fn oversized_paginated_patch_refuses_instead_of_corrupting_offsets() {
    let (t, _s, _, a, _) = fixture();
    let before = fs::read(&a).unwrap();
    assert!(sessions::patch_metadata(&a, &t.path().join("out"), "cwd", &"x".repeat(1024)).is_err());
    assert_eq!(fs::read(&a).unwrap(), before);
}
#[test]
fn referenced_rollout_cannot_be_deleted_on_its_own() {
    let (_t, s, _, a, b) = fixture();
    assert!(history::ensure_unreferenced(&s, &[a.clone()]).is_err());
    assert!(history::ensure_unreferenced(&s, &[a, b]).is_ok());
}
#[test]
fn archive_moves_all_revisions_and_restores_together() {
    let (t, s, _, a, b) = fixture();
    let rows = sessions::Scanner::default()
        .scan(&s, &CancellationToken::new())
        .unwrap()
        .sessions;
    let mut j = Journal::new(&t.path().join("store"), "archive").unwrap();
    sessions::stage_changes(
        &mut j,
        &s,
        &rows,
        "archive",
        None,
        &CancellationToken::new(),
    )
    .unwrap();
    j.commit().unwrap();
    assert!(!a.exists() && !b.exists());
    let scan = sessions::Scanner::default()
        .scan(&s, &CancellationToken::new())
        .unwrap();
    assert!(scan.warnings.is_empty());
    assert_eq!(scan.sessions[0].related_paths.len(), 1);
    assert_eq!(
        sessions::detail(&scan.sessions[0], 100000)
            .unwrap()
            .messages
            .len(),
        2
    );
}

fn unpadded() -> (
    tempfile::TempDir,
    Settings,
    String,
    PathBuf,
    PathBuf,
    PathBuf,
) {
    let (t, s, id, a, b) = fixture();
    for path in [&a, &b] {
        let old = fs::read(path).unwrap();
        let end = old.iter().position(|b| *b == b'\n').unwrap();
        let mut meta: serde_json::Value = serde_json::from_slice(&old[..end]).unwrap();
        meta["payload"]["model_provider"] = "openai".into();
        if path == &b {
            meta["payload"]["history_base"]["end_byte_offset"] =
                fs::metadata(&a).unwrap().len().into();
        }
        let mut bytes = serde_json::to_vec(&meta).unwrap();
        bytes.extend_from_slice(&old[end..]);
        fs::write(path, bytes).unwrap();
    }
    let child = uuid::Uuid::new_v4().to_string();
    let c = b
        .parent()
        .unwrap()
        .join(format!("rollout-child-{child}.jsonl"));
    fs::write(&c,format!("{}\n{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"user_message\",\"message\":\"child\"}}}}\n",json!({"type":"session_meta","payload":{"id":child,"session_id":id,"history_mode":"paginated","model_provider":"openai","cwd":"C:/project","history_base":{"thread_id":history::rollout_id(&b).unwrap(),"end_byte_offset":fs::metadata(&b).unwrap().len(),"end_ordinal_exclusive":4}}}))).unwrap();
    let db = rusqlite::Connection::open(s.codex_home.join("state.sqlite")).unwrap();
    db.execute("UPDATE threads SET model_provider='openai'", [])
        .unwrap();
    db.execute(
        "INSERT INTO threads VALUES(?1,'child','C:/project','openai',?2,2,0)",
        rusqlite::params![child, c.to_string_lossy()],
    )
    .unwrap();
    drop(db);
    let db = rusqlite::Connection::open(s.codex_home.join("thread_history_1.sqlite")).unwrap();
    db.execute_batch("CREATE TABLE thread_turns(thread_id TEXT,rollout_byte_offset INTEGER,rollout_end_byte_offset INTEGER,rollout_ordinal INTEGER,unknown TEXT);CREATE TABLE thread_history_projection_state(thread_id TEXT PRIMARY KEY,next_rollout_byte_offset INTEGER,next_rollout_ordinal INTEGER);").unwrap();
    for (id, path) in [(&id, &b), (&child, &c)] {
        let raw = fs::read(path).unwrap();
        let start = raw.iter().position(|b| *b == b'\n').unwrap() + 1;
        db.execute(
            "INSERT INTO thread_turns VALUES(?1,?2,?3,7,'keep')",
            rusqlite::params![id, start, raw.len()],
        )
        .unwrap();
        db.execute("INSERT INTO thread_turns VALUES(?1,0,NULL,0,'keep')", [id])
            .unwrap();
        db.execute(
            "INSERT INTO thread_history_projection_state VALUES(?1,?2,8)",
            rusqlite::params![id, raw.len()],
        )
        .unwrap();
    }
    (t, s, id, a, b, c)
}
fn body(path: &std::path::Path) -> Vec<u8> {
    let bytes = fs::read(path).unwrap();
    bytes[bytes.iter().position(|b| *b == b'\n').unwrap() + 1..].to_vec()
}
fn selected(s: &Settings, id: &str) -> Vec<sessions::Session> {
    let scan = sessions::Scanner::default()
        .scan(s, &CancellationToken::new())
        .unwrap();
    assert!(scan.warnings.is_empty());
    scan.sessions
        .into_iter()
        .filter(|row| row.id == id)
        .collect()
}
#[test]
fn growth_rebases_revisions_children_and_sqlite_then_restores_exact_bytes() {
    let (t, s, id, a, b, c) = unpadded();
    let rows = selected(&s, &id);
    let paths = [
        a.clone(),
        b.clone(),
        c.clone(),
        s.codex_home.join("state.sqlite"),
        s.codex_home.join("thread_history_1.sqlite"),
    ];
    let before: Vec<_> = paths.iter().map(|p| fs::read(p).unwrap()).collect();
    let bodies: Vec<_> = [&a, &b, &c].iter().map(|p| body(p)).collect();
    let store = t.path().join("store");
    let mut j = Journal::new(&store, "growth").unwrap();
    sessions::stage_changes(
        &mut j,
        &s,
        &rows,
        "provider",
        Some("p_b48e2743bb85"),
        &CancellationToken::new(),
    )
    .unwrap();
    j.commit().unwrap();
    for (i, path) in [&a, &b, &c].iter().enumerate() {
        assert_eq!(body(path), bodies[i]);
    }
    assert_eq!(fs::metadata(&a).unwrap().len(), before[0].len() as u64 + 8);
    assert_eq!(
        history::header(&b).unwrap().base_offset,
        Some(fs::metadata(&a).unwrap().len())
    );
    assert_eq!(
        history::header(&c).unwrap().base_offset,
        Some(fs::metadata(&b).unwrap().len())
    );
    assert_eq!(
        selected(&s, &history::header(&c).unwrap().id)[0].provider,
        "openai"
    );
    assert_eq!(sessions::detail(&rows[0], 100).unwrap().messages.len(), 2);
    let db = rusqlite::Connection::open(&paths[4]).unwrap();
    for path in [&b, &c] {
        let h = history::header(path).unwrap();
        let bytes = fs::read(path).unwrap();
        let boundary = bytes.iter().position(|b| *b == b'\n').unwrap() + 1;
        let got:(usize,usize,i64,String)=db.query_row("SELECT rollout_byte_offset,rollout_end_byte_offset,rollout_ordinal,unknown FROM thread_turns WHERE thread_id=?1 AND rollout_ordinal=7",[&h.id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
        assert_eq!(got, (boundary, bytes.len(), 7, "keep".into()));
        let cursor:(usize,i64)=db.query_row("SELECT next_rollout_byte_offset,next_rollout_ordinal FROM thread_history_projection_state WHERE thread_id=?1",[&h.id],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(cursor, (bytes.len(), 8));
        assert_eq!(db.query_row("SELECT rollout_byte_offset FROM thread_turns WHERE thread_id=?1 AND rollout_end_byte_offset IS NULL",[&h.id],|r|r.get::<_,i64>(0)).unwrap(),0);
    }
    drop(db);
    let operation = j.dir.file_name().unwrap().to_str().unwrap().to_owned();
    drop(j);
    easy_switch_core::journal::restore(&store, &operation, &[s.codex_home.clone(), store.clone()])
        .unwrap();
    for (path, expected) in paths.iter().zip(before) {
        assert_eq!(fs::read(path).unwrap(), expected);
    }
}
#[test]
fn project_growth_rebases_unselected_descendant_with_digit_growth() {
    let (t, s, id, a, b, c) = unpadded();
    let rows = selected(&s, &id);
    let old_c = fs::metadata(&c).unwrap().len();
    let original = body(&c);
    let mut j = Journal::new(&t.path().join("store"), "project").unwrap();
    sessions::stage_changes(
        &mut j,
        &s,
        &rows,
        "migrate",
        Some(&format!("C:/{}", "x".repeat(1200))),
        &CancellationToken::new(),
    )
    .unwrap();
    j.commit().unwrap();
    assert!(fs::metadata(&c).unwrap().len() > old_c);
    assert_eq!(body(&c), original);
    assert_eq!(
        history::header(&b).unwrap().base_offset,
        Some(fs::metadata(&a).unwrap().len())
    );
    assert_eq!(
        history::header(&c).unwrap().base_offset,
        Some(fs::metadata(&b).unwrap().len())
    );
    let child = selected(&s, &history::header(&c).unwrap().id);
    assert_eq!(child[0].cwd, "C:/project");
    assert_eq!(sessions::detail(&child[0], 100).unwrap().messages.len(), 3);
}
#[test]
fn growth_partial_commit_rolls_back_files_and_offset_database() {
    let (t, s, id, a, b, c) = unpadded();
    let paths = [
        a,
        b,
        c,
        s.codex_home.join("state.sqlite"),
        s.codex_home.join("thread_history_1.sqlite"),
    ];
    let before: Vec<_> = paths.iter().map(|p| fs::read(p).unwrap()).collect();
    let mut j = Journal::new(&t.path().join("store"), "fault").unwrap();
    sessions::stage_changes(
        &mut j,
        &s,
        &selected(&s, &id),
        "provider",
        Some("p_b48e2743bb85"),
        &CancellationToken::new(),
    )
    .unwrap();
    assert!(j.commit_with_fault(Some(3)).is_err());
    for (p, old) in paths.iter().zip(before) {
        assert_eq!(fs::read(p).unwrap(), old);
    }
}
#[test]
fn invalid_projection_offset_refuses_before_commit() {
    let (t, s, id, a, b, c) = unpadded();
    let before = [
        fs::read(&a).unwrap(),
        fs::read(&b).unwrap(),
        fs::read(&c).unwrap(),
    ];
    let db = rusqlite::Connection::open(s.codex_home.join("thread_history_1.sqlite")).unwrap();
    db.execute(
        "UPDATE thread_history_projection_state SET next_rollout_byte_offset=2",
        [],
    )
    .unwrap();
    drop(db);
    let mut j = Journal::new(&t.path().join("store"), "invalid").unwrap();
    assert!(
        sessions::stage_changes(
            &mut j,
            &s,
            &selected(&s, &id),
            "provider",
            Some("p_b48e2743bb85"),
            &CancellationToken::new()
        )
        .is_err()
    );
    for (p, old) in [&a, &b, &c].iter().zip(before) {
        assert_eq!(fs::read(p).unwrap(), old);
    }
}
#[test]
fn unknown_projection_column_refuses_before_commit() {
    let (t, s, id, a, _, _) = unpadded();
    let before = fs::read(&a).unwrap();
    let db = rusqlite::Connection::open(s.codex_home.join("thread_history_1.sqlite")).unwrap();
    db.execute(
        "ALTER TABLE thread_turns ADD COLUMN future_byte_offset INTEGER",
        [],
    )
    .unwrap();
    drop(db);
    let mut j = Journal::new(&t.path().join("store"), "unknown").unwrap();
    assert!(
        sessions::stage_changes(
            &mut j,
            &s,
            &selected(&s, &id),
            "provider",
            Some("p_b48e2743bb85"),
            &CancellationToken::new()
        )
        .unwrap_err()
        .to_string()
        .contains("未知")
    );
    assert_eq!(fs::read(&a).unwrap(), before);
}
