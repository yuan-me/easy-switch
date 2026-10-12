//! Opt-in synthetic benchmark: never opens the user's Codex home or stops Codex.
use easy_switch_core::{Settings, journal::Journal, sessions};
use serde_json::json;
use std::{fs, time::Instant};
use tokio_util::sync::CancellationToken;

#[test]
#[ignore = "synthetic repair benchmark; run --release --ignored --nocapture"]
fn repair_performance() {
    for run in 1..=3 {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let folder = home.join("sessions");
        fs::create_dir_all(&folder).unwrap();
        let settings = Settings {
            codex_home: home.clone(),
            ..Default::default()
        };
        let mut db = rusqlite::Connection::open(home.join("state.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE threads(id TEXT PRIMARY KEY,title TEXT,cwd TEXT,model_provider TEXT,rollout_path TEXT,updated_at INTEGER,archived INTEGER);
            CREATE TABLE thread_turns(thread_id TEXT,rollout_byte_offset INTEGER,rollout_end_byte_offset INTEGER);
            CREATE INDEX turns_thread ON thread_turns(thread_id);
            CREATE TABLE thread_history_projection_state(thread_id TEXT PRIMARY KEY,next_rollout_byte_offset INTEGER);").unwrap();
        let tx = db.transaction().unwrap();
        let event = format!(
            "{}\n",
            json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"synthetic repair benchmark ".repeat(40)}],"unknown":{"keep":[1,2,3]}}})
        );
        let body = event.repeat(1000);
        let mut paths = Vec::new();
        for _ in 0..24 {
            let id = uuid::Uuid::new_v4().to_string();
            let path = folder.join(format!("rollout-{id}.jsonl"));
            let meta = format!(
                "{}\n",
                json!({"type":"session_meta","payload":{"id":id,"session_id":id,"history_mode":"paginated","model_provider":"openai","cwd":"C:/synthetic"}})
            );
            fs::write(&path, format!("{meta}{body}")).unwrap();
            tx.execute(
                "INSERT INTO threads VALUES(?1,'synthetic','C:/synthetic','openai',?2,1,0)",
                rusqlite::params![id, path.to_string_lossy()],
            )
            .unwrap();
            for row in 0..1000 {
                tx.execute(
                    "INSERT INTO thread_turns VALUES(?1,?2,?3)",
                    rusqlite::params![
                        id,
                        meta.len() + row * event.len(),
                        meta.len() + (row + 1) * event.len()
                    ],
                )
                .unwrap();
            }
            tx.execute(
                "INSERT INTO thread_history_projection_state VALUES(?1,?2)",
                rusqlite::params![id, meta.len() + body.len()],
            )
            .unwrap();
            paths.push((id, path));
        }
        tx.commit().unwrap();
        drop(db);
        let ct = CancellationToken::new();
        let start = Instant::now();
        let scan = sessions::Scanner::default().scan(&settings, &ct).unwrap();
        let scan_ms = start.elapsed().as_secs_f64() * 1000.;
        assert!(scan.warnings.is_empty());
        assert_eq!(scan.sessions.len(), 24);
        let mut journal = Journal::new(&temp.path().join("store"), "synthetic benchmark").unwrap();
        let start = Instant::now();
        sessions::stage_changes(
            &mut journal,
            &settings,
            &scan.sessions,
            "provider",
            Some("p_synthetic_longer_provider"),
            &ct,
        )
        .unwrap();
        let stage_ms = start.elapsed().as_secs_f64() * 1000.;
        let start = Instant::now();
        journal.commit().unwrap();
        let commit_ms = start.elapsed().as_secs_f64() * 1000.;
        let db = rusqlite::Connection::open(home.join("state.sqlite")).unwrap();
        for (id, path) in &paths {
            let after = fs::read_to_string(path).unwrap();
            let (meta, actual_body) = after.split_once('\n').unwrap();
            assert_eq!(actual_body, body);
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(meta).unwrap()["payload"]["model_provider"],
                "p_synthetic_longer_provider"
            );
            let bounds: (usize,usize) = db.query_row("SELECT MIN(rollout_byte_offset),MAX(rollout_end_byte_offset) FROM thread_turns WHERE thread_id=?1", [id], |r| Ok((r.get(0)?,r.get(1)?))).unwrap();
            assert_eq!(bounds, (meta.len() + 1, after.len()));
        }
        drop(db);
        drop(journal);
        let mut repeat = Journal::new(&temp.path().join("store"), "already repaired").unwrap();
        let start = Instant::now();
        sessions::stage_changes(
            &mut repeat,
            &settings,
            &scan.sessions,
            "provider",
            Some("p_synthetic_longer_provider"),
            &ct,
        )
        .unwrap();
        assert!(repeat.manifest.files.is_empty());
        repeat.commit().unwrap();
        let repeat_ms = start.elapsed().as_secs_f64() * 1000.;
        println!(
            "{}",
            json!({"run":run,"sessions":24,"events":24000,"bytes":body.len()*24,"scan_ms":scan_ms,"stage_ms":stage_ms,"commit_ms":commit_ms,"total_ms":scan_ms+stage_ms+commit_ms,"repeat_ms":repeat_ms})
        );
    }
}
