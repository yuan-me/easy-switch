//! Same-user offline restore probe. Originals are only read; plaintext stays in the supplied scratch directory.
use anyhow::{Context, ensure};
use easy_switch_core::{
    Result, crypto,
    journal::{self, Manifest},
};
use std::{fs, path::PathBuf};
fn main() {
    match run() {
        Ok(n) => println!(
            "{}",
            serde_json::json!({"status":"passed","restoredFiles":n,"originalsModified":false})
        ),
        Err(_) => {
            println!(
                "{}",
                serde_json::json!({"status":"failed","message":"No complete legacy backup could be restored in isolation"})
            );
            std::process::exit(1);
        }
    }
}
fn run() -> Result<usize> {
    let args: Vec<_> = std::env::args_os().collect();
    ensure!(
        args.len() == 3,
        "legacy root and empty scratch directory required"
    );
    let legacy = PathBuf::from(&args[1]);
    let scratch = PathBuf::from(&args[2]);
    ensure!(!scratch.exists(), "scratch must be new");
    fs::create_dir_all(&scratch)?;
    for info in journal::list(&legacy.join("operations"), true)? {
        let source: Manifest = serde_json::from_slice(&fs::read(
            legacy
                .join("operations")
                .join(&info.id)
                .join("manifest.json"),
        )?)?;
        if source.state != "Complete" || source.files.is_empty() || source.files.len() > 200 {
            continue;
        }
        let root = scratch.join(&info.id);
        let dir = root.join("operations/probe");
        fs::create_dir_all(&dir)?;
        let attempt = (|| -> Result<usize> {
            let mut clone = source.clone();
            for (i, c) in clone.files.iter_mut().enumerate() {
                let original = c.path.clone();
                c.path = root.join(format!(
                    "file-{i}.{}",
                    original
                        .extension()
                        .and_then(|v| v.to_str())
                        .unwrap_or("bin")
                ));
                if let Some(expected) = &c.after_hash {
                    if let Some(stage) = c.stage.as_ref().filter(|p| p.is_file()) {
                        let mut out = fs::File::create(&c.path)?;
                        crypto::decrypt_file(stage, &mut out)?;
                        out.sync_all()?;
                    } else {
                        ensure!(
                            journal::hash(&original)?.as_ref() == Some(expected),
                            "post-image unavailable"
                        );
                        fs::copy(&original, &c.path)?;
                    }
                    ensure!(
                        journal::hash(&c.path)?.as_ref() == Some(expected),
                        "post-image mismatch"
                    );
                }
                if let Some(payload) = &c.backup {
                    let p = dir.join(format!("before-{i}.bin"));
                    fs::copy(payload, &p)?;
                    c.backup = Some(p)
                }
                c.stage = None;
            }
            fs::write(
                dir.join("manifest.json"),
                serde_json::to_vec_pretty(&clone)?,
            )?;
            journal::restore(&root, "probe", &[root.clone()])?;
            for c in &clone.files {
                ensure!(
                    journal::hash(&c.path)? == c.before_hash,
                    "restored hash mismatch"
                )
            }
            Ok(clone.files.len())
        })();
        if let Ok(n) = attempt {
            return Ok(n);
        }
    }
    None.context("no restorable legacy operation")
}
