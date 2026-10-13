use easy_switch_core::{
    crypto,
    journal::{self, Journal},
};
use std::{fs, io::Cursor};

fn original() -> Vec<u8> {
    (0..196_613).map(|i| (i % 251) as u8).collect()
}

#[test]
fn delta_staging_keeps_full_backup_and_restores_growth_shrink_insert_delete_and_edges() {
    let large_replacement = vec![255; 1_048_583];
    for (start, end, replacement) in [
        (0, 15, b"longer replacement at the start".as_slice()),
        (65_531, 65_550, b"short".as_slice()),
        (65_536, 65_536, b"insert".as_slice()),
        (70_000, 70_123, b"".as_slice()),
        (196_613, 196_613, b"append".as_slice()),
        (131_077, 196_613, b"".as_slice()),
        (65_531, 70_123, large_replacement.as_slice()),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("original");
        let source = temp.path().join("next");
        let store = temp.path().join("store");
        let before = original();
        let after = [&before[..start], replacement, &before[end..]].concat();
        fs::write(&path, &before).unwrap();
        fs::write(&source, &after).unwrap();
        let mut j = Journal::new(&store, "delta").unwrap();
        j.stage_file(&path, &source).unwrap();
        let record = &j.manifest.files[0];
        assert!(serde_json::to_value(record).unwrap()["patch"].is_object());
        let mut backup = Vec::new();
        crypto::decrypt_file(record.backup.as_ref().unwrap(), &mut backup).unwrap();
        assert_eq!(backup, before);
        let mut staged = Vec::new();
        crypto::decrypt_file(record.stage.as_ref().unwrap(), &mut staged).unwrap();
        assert!(staged.len() <= replacement.len());
        j.commit().unwrap();
        assert_eq!(fs::read(&path).unwrap(), after);
        let id = j.dir.file_name().unwrap().to_str().unwrap().to_owned();
        // Recovery continues to use the existing full-file backup contract.
        j.manifest.state = "Applying".into();
        let mut saved = serde_json::to_value(&j.manifest).unwrap();
        saved["files"][0].as_object_mut().unwrap().remove("patch");
        fs::write(
            j.dir.join("manifest.json"),
            serde_json::to_vec(&saved).unwrap(),
        )
        .unwrap();
        drop(j);
        journal::restore(&store, &id, &[temp.path().to_path_buf()]).unwrap();
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}

#[test]
fn delta_rejects_invalid_ranges_corruption_and_external_modifications() {
    for fault in [
        "range",
        "ciphertext",
        "source",
        "late-stage",
        "partial-commit",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("original");
        let source = temp.path().join("next");
        let before = original();
        let after = [b"metadata".as_slice(), &before[3..]].concat();
        fs::write(&path, &before).unwrap();
        fs::write(&source, &after).unwrap();
        let mut j = Journal::new(&temp.path().join("store"), "delta fault").unwrap();
        j.stage_file(&path, &source).unwrap();
        let stage = j.manifest.files[0].stage.clone().unwrap();
        match fault {
            "range" => {
                let mut m = serde_json::to_value(&j.manifest).unwrap();
                m["files"][0]["patch"]["end"] = u64::MAX.into();
                j.manifest = serde_json::from_value(m).unwrap();
            }
            "ciphertext" => fs::write(&stage, b"broken").unwrap(),
            "source" => fs::write(&path, b"external edit").unwrap(),
            _ => {}
        }
        let result = if fault == "partial-commit" {
            j.commit_with_fault(Some(0))
        } else {
            j.commit_with_progress(&|p| {
                if fault == "late-stage" && p.detail.starts_with("写入并校验") {
                    crypto::encrypt_reader(Cursor::new(b"wrong delta"), &stage).unwrap();
                }
            })
        };
        assert!(result.is_err(), "fault {fault} was accepted");
        assert_eq!(
            fs::read(&path).unwrap(),
            if fault == "source" {
                b"external edit".to_vec()
            } else {
                before
            }
        );
    }
}

#[test]
fn unchanged_files_and_small_replacements_keep_existing_behavior() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("original");
    let source = temp.path().join("next");
    fs::write(&path, b"old").unwrap();
    fs::write(&source, b"old").unwrap();
    let mut j = Journal::new(&temp.path().join("store"), "small").unwrap();
    j.stage_file(&path, &source).unwrap();
    assert!(j.manifest.files.is_empty());
    fs::write(&source, b"new").unwrap();
    j.stage_file(&path, &source).unwrap();
    assert!(
        serde_json::to_value(&j.manifest.files[0])
            .unwrap()
            .get("patch")
            .is_none()
    );
    j.commit().unwrap();
    assert_eq!(fs::read(path).unwrap(), b"new");
}
