use anyhow::{Context, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let file = args
        .get(1)
        .context("usage: verify_bundle installer.exe tauri.conf.json")?;
    let config = args.get(2).context("missing config")?;
    let config: serde_json::Value = serde_json::from_slice(&std::fs::read(config)?)?;
    let key = STANDARD.decode(
        config["plugins"]["updater"]["pubkey"]
            .as_str()
            .context("missing public key")?,
    )?;
    let key = minisign_verify::PublicKey::decode(std::str::from_utf8(&key)?)?;
    let encoded = std::fs::read_to_string(format!("{file}.sig"))?;
    let signature = STANDARD.decode(encoded.trim())?;
    let signature = minisign_verify::Signature::decode(std::str::from_utf8(&signature)?)?;
    let mut bytes = std::fs::read(file)?;
    key.verify(&bytes, &signature, false)?;
    let version = config["version"].as_str().context("missing version")?;
    ensure!(
        signature
            .trusted_comment()
            .split('\t')
            .any(|field| field == format!("version:{version}")),
        "signed version mismatch"
    );
    ensure!(!bytes.is_empty(), "empty artifact");
    bytes[0] ^= 1;
    ensure!(
        key.verify(&bytes, &signature, false).is_err(),
        "tampered artifact accepted"
    );
    println!("Installer signature, signed version and tamper rejection verified ({version}).");
    Ok(())
}
