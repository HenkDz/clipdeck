use super::{
    native,
    stage::{self, Stage},
};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub struct Job {
    pub stage: Stage,
    pub target: PathBuf,
    pub pid: u32,
    pub created: u64,
    pub old_hash: String,
    pub nonce: String,
}

pub fn read(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let mut bytes = vec![];
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("Update job exceeds its size limit".into());
    }
    Ok(bytes)
}
impl Job {
    pub fn create(stage: Stage) -> Result<Self, String> {
        let target = std::env::current_exe()
            .and_then(|p| p.canonicalize())
            .map_err(|e| e.to_string())?;
        let parent = native::Parent::open(std::process::id())?;
        let nonce = Self::nonce(&stage.dir)?;
        let job = Self {
            old_hash: stage::hash(&target)?.1,
            target,
            pid: std::process::id(),
            created: parent.created()?,
            nonce,
            stage,
        };
        let mut table = toml::Table::new();
        table.insert(
            "target".into(),
            job.target
                .to_str()
                .ok_or("Unsupported application path")?
                .into(),
        );
        table.insert("pid".into(), i64::from(job.pid).into());
        table.insert("created".into(), (job.created as i64).into());
        table.insert("old_hash".into(), job.old_hash.clone().into());
        fs::write(job.path(), table.to_string()).map_err(|e| e.to_string())?;
        stage::verify(&job.stage.dir.join("download.exe"), &job.stage.release)?;
        Self::copy_new(&job.stage.dir.join("download.exe"), &job.candidate())?;
        stage::verify(&job.candidate(), &job.stage.release)?;
        Self::copy_new(&job.target, &job.stage.dir.join("helper.exe"))?;
        Ok(job)
    }
    fn copy_new(from: &Path, to: &Path) -> Result<(), String> {
        let mut output = fs::File::options()
            .write(true)
            .create_new(true)
            .open(to)
            .map_err(|e| e.to_string())?;
        std::io::copy(
            &mut fs::File::open(from).map_err(|e| e.to_string())?,
            &mut output,
        )
        .map_err(|e| e.to_string())?;
        output.flush().map_err(|e| e.to_string())?;
        output.sync_all().map_err(|e| e.to_string())
    }
    fn nonce(dir: &Path) -> Result<String, String> {
        let nonce = dir
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("Invalid update folder")?;
        if nonce.len() != 32
            || !nonce
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("Invalid update folder".into());
        }
        Ok(nonce.into())
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let path = path.canonicalize().map_err(|e| e.to_string())?;
        if path.file_name().is_none_or(|name| name != "job.toml") {
            return Err("Invalid update job filename".into());
        }
        let stage = Stage::load(path.parent().ok_or("Invalid update job")?)?;
        let nonce = Self::nonce(&stage.dir)?;
        let bytes = read(&path, 8192)?;
        let table = std::str::from_utf8(&bytes)
            .map_err(|e| e.to_string())?
            .parse::<toml::Table>()
            .map_err(|e| e.to_string())?;
        let string = |key| {
            table
                .get(key)
                .and_then(toml::Value::as_str)
                .ok_or("Invalid update job field")
        };
        let integer = |key| {
            table
                .get(key)
                .and_then(toml::Value::as_integer)
                .filter(|n| *n > 0)
                .ok_or("Invalid update process identity")
        };
        let target = PathBuf::from(string("target")?)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let pid = u32::try_from(integer("pid")?).map_err(|_| "Invalid process ID")?;
        let created = integer("created")? as u64;
        let old_hash = string("old_hash")?.to_owned();
        if old_hash.len() != 64 || !old_hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Invalid previous executable hash".into());
        }
        Ok(Self {
            stage,
            target,
            pid,
            created,
            old_hash,
            nonce,
        })
    }
    pub fn parent(&self) -> Result<native::Parent, String> {
        let parent = native::Parent::open(self.pid)?;
        if parent.created()? != self.created
            || parent.image()? != self.target
            || stage::hash(&self.target)?.1 != self.old_hash
        {
            return Err("The application or its process changed before the update".into());
        }
        Ok(parent)
    }
    pub fn path(&self) -> PathBuf {
        self.stage.dir.join("job.toml")
    }
    pub fn candidate(&self) -> PathBuf {
        self.target
            .with_file_name(format!(".clipdeck-update-{}.exe", self.nonce))
    }
    pub fn backup(&self) -> PathBuf {
        self.target
            .with_file_name(format!(".clipdeck-previous-{}.exe", self.nonce))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn staging_names_and_reads_are_bounded() {
        assert!(Job::nonce(Path::new("0123456789abcdef0123456789abcdef")).is_ok());
        for name in [
            "../outside",
            "not-a-nonce",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "0123456789abcdef0123456789abcde/",
        ] {
            assert!(Job::nonce(Path::new(name)).is_err());
        }
        let path = std::env::temp_dir().join(format!(
            "clipdeck-update-limit-{}",
            super::super::crypto::nonce().unwrap()
        ));
        fs::write(&path, [0; 65]).unwrap();
        assert!(read(&path, 64).is_err());
        assert_eq!(read(&path, 65).unwrap().len(), 65);
        fs::remove_file(path).unwrap();
    }
}
