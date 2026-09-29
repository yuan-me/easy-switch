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
