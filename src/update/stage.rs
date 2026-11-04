use super::{Release, crypto, http, native};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

pub fn root() -> Result<PathBuf, String> {
    std::env::var_os("LOCALAPPDATA")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(dirs::data_local_dir)
        .map(|p| p.join("Clipdeck/updates"))
        .ok_or("LOCALAPPDATA is unavailable".into())
}

pub fn hash(path: &Path) -> Result<(u64, String), String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut size = 0;
    let mut bytes = [0; 65536];
    loop {
        let read = file.read(&mut bytes).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        size += read as u64;
        hash.update(&bytes[..read]);
        if size > 128 * 1024 * 1024 {
            return Err("Executable exceeds the update size limit".into());
        }
    }
    Ok((
        size,
        hash.finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    ))
}
pub fn verify(path: &Path, release: &Release) -> Result<(), String> {
    let (size, hash) = hash(path)?;
    if size != release.manifest.size || hash != release.manifest.hash {
        return Err(
            "Update download is incomplete or its checksum is wrong. Nothing was installed.".into(),
        );
    }
    if native::version(path)? != release.manifest.version {
        return Err("Executable version does not match the signed update".into());
    }
    Ok(())
}
#[derive(Clone)]
pub struct Stage {
    pub dir: PathBuf,
    pub release: Release,
}
impl Stage {
    pub fn download(release: Release, progress: impl FnMut(u64)) -> Result<Self, String> {
        let dir = root()?.join(crypto::nonce()?);
        fs::create_dir_all(dir.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::create_dir(&dir).map_err(|e| e.to_string())?;
        fs::write(dir.join("update.toml"), &release.bytes).map_err(|e| e.to_string())?;
        fs::write(dir.join("update.sig"), &release.signature).map_err(|e| e.to_string())?;
        let partial = dir.join("download.part");
        let result = (|| {
            let mut file = File::options()
                .write(true)
                .create_new(true)
                .open(&partial)
                .map_err(|e| e.to_string())?;
            let size = http::get(
                &release.manifest.asset_url(),
                &mut file,
                release.manifest.size,
                progress,
            )?;
            file.sync_all().map_err(|e| e.to_string())?;
            drop(file);
            if size != release.manifest.size {
                return Err("Update download was interrupted".into());
            }
            verify(&partial, &release)?;
            fs::rename(&partial, dir.join("download.exe")).map_err(|e| e.to_string())?;
            Ok(Self { dir, release })
        })();
        if result.is_err() {
            let _ = fs::remove_file(partial);
        }
        result
    }
    pub fn load(dir: &Path) -> Result<Self, String> {
        let dir = dir.canonicalize().map_err(|e| e.to_string())?;
        let root = root()?.canonicalize().map_err(|e| e.to_string())?;
        if dir.parent() != Some(root.as_path()) {
            return Err("Update job is outside the private staging folder".into());
        }
        let bytes = super::job::read(&dir.join("update.toml"), 16_384)?;
        let signature = super::job::read(&dir.join("update.sig"), 64)?;
        let manifest = super::manifest::Manifest::verified(&bytes, &signature)?;
        Ok(Self {
            dir,
            release: Release {
                manifest,
                bytes,
                signature,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incomplete_tampered_and_wrong_architecture_downloads_are_rejected() {
        let path = std::env::temp_dir().join(format!(
            "clipdeck-update-bytes-{}",
            crypto::nonce().unwrap()
        ));
        fs::write(&path, b"abc").unwrap();
        assert_eq!(
            hash(&path).unwrap(),
            (
                3,
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into()
            )
        );
        let manifest = super::super::manifest::Manifest::verified(
            include_bytes!("../../assets/update-test.toml"),
            include_bytes!("../../assets/update-test.toml.sig"),
        )
        .unwrap();
        let mut release = Release {
            manifest,
            bytes: vec![],
            signature: vec![],
        };
        assert!(verify(&path, &release).unwrap_err().contains("incomplete"));
        let mut bytes = vec![0u8; 65536];
        fs::write(&path, &bytes).unwrap();
        assert!(verify(&path, &release).unwrap_err().contains("checksum"));
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&128u32.to_le_bytes());
        bytes[128..134].copy_from_slice(b"PE\0\0\x4c\x01");
        fs::write(&path, bytes).unwrap();
        release.manifest.hash = hash(&path).unwrap().1;
        assert!(verify(&path, &release).unwrap_err().contains("x64"));
        fs::remove_file(path).unwrap();
    }
}
