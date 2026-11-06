use anyhow::{Result, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

#[cfg(windows)]
fn transform(data: &[u8], encrypt: bool) -> Result<Vec<u8>> {
    use windows::Win32::{
        Foundation::{HLOCAL, LocalFree},
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len().try_into()?,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )?;
        } else {
            CryptUnprotectData(
                &input,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )?;
        }
        let result = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(Some(HLOCAL(output.pbData as *mut _)));
        Ok(result)
    }
}
#[cfg(not(windows))]
static SESSION_KEY: std::sync::OnceLock<[u8; 32]> = std::sync::OnceLock::new();

#[cfg(not(windows))]
pub fn set_session_key(key: [u8; 32]) -> Result<()> {
    if let Err(key) = SESSION_KEY.set(key) {
        ensure!(
            SESSION_KEY.get() == Some(&key),
            "本进程已使用另一把操作密钥"
        );
    }
    Ok(())
}

#[cfg(not(windows))]
fn cipher() -> Result<aes_gcm::Aes256Gcm> {
    use aes_gcm::KeyInit;
    let key = SESSION_KEY
        .get()
        .ok_or_else(|| anyhow::anyhow!("操作加密密钥未初始化"))?;
    Ok(aes_gcm::Aes256Gcm::new(key.into()))
}

#[cfg(not(windows))]
fn transform(data: &[u8], encrypt: bool) -> Result<Vec<u8>> {
    use aes_gcm::{Nonce, aead::Aead};
    use rand::RngCore;
    let cipher = cipher()?;
    if encrypt {
        let mut nonce = [0u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let encrypted = cipher
            .encrypt(Nonce::from_slice(&nonce), data)
            .map_err(|_| anyhow::anyhow!("凭证加密失败"))?;
        Ok([b"CSG0".as_slice(), &nonce, &encrypted].concat())
    } else {
        ensure!(
            data.len() >= 32 && &data[..4] == b"CSG0",
            "不支持的凭证格式；不能读取 Windows DPAPI 备份"
        );
        cipher
            .decrypt(Nonce::from_slice(&data[4..16]), &data[16..])
            .map_err(|_| anyhow::anyhow!("凭证认证失败"))
    }
}
pub fn protect(text: &str) -> Result<String> {
    Ok(STANDARD.encode(transform(text.as_bytes(), true)?))
}
pub fn unprotect(text: &str) -> Result<String> {
    Ok(String::from_utf8(transform(
        &STANDARD.decode(text)?,
        false,
    )?)?)
}
pub fn encrypt_file(source: &Path, dest: &Path) -> Result<()> {
    encrypt_reader(File::open(source)?, dest)
}
#[cfg(windows)]
pub fn encrypt_reader(mut input: impl Read, dest: &Path) -> Result<()> {
    let mut out = crate::store::private_file(dest, false)?;
    out.write_all(b"CSB1")?;
    let mut chunk = vec![0u8; 1024 * 1024];
    loop {
        let n = input.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        let enc = transform(&chunk[..n], true)?;
        out.write_all(&(enc.len() as i32).to_le_bytes())?;
        out.write_all(&enc)?;
    }
    out.write_all(&0i32.to_le_bytes())?;
    out.sync_all()?;
    Ok(())
}
#[cfg(windows)]
pub fn decrypt_file(source: &Path, mut out: impl Write) -> Result<()> {
    let mut input = File::open(source)?;
    let mut magic = [0; 4];
    input.read_exact(&mut magic)?;
    if &magic != b"CSB1" {
        let mut bytes = magic.to_vec();
        input.read_to_end(&mut bytes)?;
        out.write_all(&transform(&bytes, false)?)?;
        return Ok(());
    }
    loop {
        let mut b = [0; 4];
        input.read_exact(&mut b)?;
        let n = i32::from_le_bytes(b);
        if n == 0 {
            break;
        }
        ensure!(n > 0 && n <= 2 * 1024 * 1024, "加密备份分块损坏");
        let mut bytes = vec![0; n as usize];
        input.read_exact(&mut bytes)?;
        out.write_all(&transform(&bytes, false)?)?;
    }
    let mut extra = [0; 1];
    if input.read(&mut extra)? != 0 {
        bail!("备份尾部有未知数据");
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn encrypt_reader(mut input: impl Read, dest: &Path) -> Result<()> {
    use aes_gcm::{
        Nonce,
        aead::{Aead, Payload},
    };
    use rand::RngCore;
    let cipher = cipher()?;
    let mut out = crate::store::private_file(dest, false)?;
    let mut prefix = [0u8; 8];
    rand::rngs::OsRng.fill_bytes(&mut prefix);
    out.write_all(b"CSG1")?;
    out.write_all(&prefix)?;
    let mut chunk = vec![0u8; 1024 * 1024];
    for index in 0..=u32::MAX {
        let n = input.read(&mut chunk)?;
        let mut nonce = [0u8; 12];
        nonce[..8].copy_from_slice(&prefix);
        nonce[8..].copy_from_slice(&index.to_be_bytes());
        let mut aad = [0u8; 9];
        aad[..4].copy_from_slice(b"CSG1");
        aad[4..8].copy_from_slice(&index.to_be_bytes());
        aad[8] = u8::from(n == 0);
        let encrypted = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &chunk[..n],
                    aad: &aad,
                },
            )
            .map_err(|_| anyhow::anyhow!("备份加密失败"))?;
        out.write_all(&(n as u32).to_le_bytes())?;
        out.write_all(&encrypted)?;
        if n == 0 {
            out.sync_all()?;
            return Ok(());
        }
    }
    bail!("备份过大")
}

#[cfg(not(windows))]
pub fn decrypt_file(source: &Path, mut out: impl Write) -> Result<()> {
    use aes_gcm::{
        Nonce,
        aead::{Aead, Payload},
    };
    let cipher = cipher()?;
    let mut input = File::open(source)?;
    let mut magic = [0u8; 4];
    input.read_exact(&mut magic)?;
    ensure!(
        &magic == b"CSG1",
        "不支持的备份格式；不能读取 Windows DPAPI 备份"
    );
    let mut prefix = [0u8; 8];
    input.read_exact(&mut prefix)?;
    for index in 0..=u32::MAX {
        let mut length = [0u8; 4];
        input.read_exact(&mut length)?;
        let n = u32::from_le_bytes(length) as usize;
        ensure!(n <= 1024 * 1024, "加密备份分块损坏");
        let mut encrypted = vec![0; n + 16];
        input.read_exact(&mut encrypted)?;
        let mut nonce = [0u8; 12];
        nonce[..8].copy_from_slice(&prefix);
        nonce[8..].copy_from_slice(&index.to_be_bytes());
        let mut aad = [0u8; 9];
        aad[..4].copy_from_slice(b"CSG1");
        aad[4..8].copy_from_slice(&index.to_be_bytes());
        aad[8] = u8::from(n == 0);
        let plain = cipher
            .decrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &encrypted,
                    aad: &aad,
                },
            )
            .map_err(|_| anyhow::anyhow!("备份认证失败"))?;
        if n == 0 {
            let mut extra = [0u8; 1];
            ensure!(input.read(&mut extra)? == 0, "备份尾部有未知数据");
            return Ok(());
        }
        out.write_all(&plain)?;
    }
    bail!("备份过大")
}
