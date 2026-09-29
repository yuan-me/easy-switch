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
fn transform(_: &[u8], _: bool) -> Result<Vec<u8>> {
    bail!("凭证与旧备份仅支持原 Windows 用户的 DPAPI");
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
pub fn encrypt_reader(mut input: impl Read, dest: &Path) -> Result<()> {
    let mut out = File::create(dest)?;
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
