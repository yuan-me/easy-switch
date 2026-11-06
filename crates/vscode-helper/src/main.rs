use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use easy_switch_helper::Request;
use serde_json::json;
use std::io::{Read, Write};

fn main() {
    let mut secrets = Vec::new();
    let result = run(&mut secrets);
    if let Err(error) = result {
        let mut message = error.to_string();
        for secret in secrets {
            if !secret.is_empty() {
                message = message.replace(&secret, "[已隐藏]");
            }
        }
        println!("{}", json!({"type":"error","message":message}));
        std::process::exit(1);
    }
}

fn run(secrets: &mut Vec<String>) -> Result<()> {
    ensure!(
        cfg!(target_os = "linux"),
        "切换工具仅支持 Linux Remote-SSH 扩展宿主"
    );
    let mut input = String::new();
    std::io::stdin()
        .take(1_048_577)
        .read_to_string(&mut input)?;
    ensure!(input.len() <= 1_048_576, "请求超过大小限制");
    let req: Request =
        serde_json::from_str(&input).map_err(|_| anyhow::anyhow!("切换请求格式无效"))?;
    if let Some(key) = &req.api_key {
        secrets.push(key.clone());
    }
    if let Some(key) = &req.journal_key {
        secrets.push(key.clone());
    }
    if req.action != "inspect" {
        let encoded = req
            .journal_key
            .as_deref()
            .context("缺少操作备份密钥；请使用原客户端恢复")?;
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| anyhow::anyhow!("操作备份密钥格式无效"))?;
        let key: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("操作备份密钥长度无效"))?;
        #[cfg(not(windows))]
        easy_switch_core::crypto::set_session_key(key)?;
        #[cfg(windows)]
        let _ = key;
    }
    let result = easy_switch_helper::execute(&req, &|p| {
        println!(
            "{}",
            json!({"type":"progress","stage":p.stage,"detail":p.detail,"completed":p.completed,"total":p.total})
        );
        let _ = std::io::stdout().flush();
    })?;
    let mut result = result;
    result["type"] = json!("result");
    println!("{result}");
    Ok(())
}
