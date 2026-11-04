use super::{
    database,
    job::Job,
    native,
    stage::{self, Stage},
};
use std::{
    fs,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::OnceLock,
    time::Duration,
};

static INCOMING: OnceLock<PathBuf> = OnceLock::new();
fn marker(path: &Path, value: &str) -> Result<(), String> {
    use std::io::Write;
    if path.exists() {
        return Err("Update acknowledgement already exists".into());
    }
    let temporary = path.with_extension("tmp");
    let mut file = fs::File::options()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    file.write_all(value.as_bytes())
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    fs::rename(temporary, path).map_err(|e| e.to_string())
}
fn launch(path: &Path) -> Command {
    let mut command = Command::new(path);
    command.creation_flags(0x08000000);
    command
}
pub fn start(stage: Stage) -> Result<(), String> {
    let job = Job::create(stage)?;
    let mut helper = launch(&job.stage.dir.join("helper.exe"))
        .arg("--apply-update")
        .arg(job.path())
        .spawn()
        .map_err(|e| e.to_string())?;
    for _ in 0..100 {
        if job.stage.dir.join("helper.ready").exists() {
            return Ok(());
        }
        if helper.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Err(fs::read_to_string(job.stage.dir.join("helper.error"))
                .unwrap_or_else(|_| "The update helper could not start".into()));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // The helper waits for the parent and cannot replace a running executable.
    let _ = helper.kill();
    let _ = helper.wait();
    Err("Update helper timed out; Clipdeck is still running".into())
}
pub fn bootstrap() -> Result<bool, String> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let Some(mode) = args.first() else {
        return native::updating();
    };
    #[cfg(debug_assertions)]
    if mode == "--test-update" {
        if args.len() != 2 {
            return Err("Invalid test update arguments".into());
        }
        start(Stage::load(Path::new(&args[1]))?)?;
        // Allows the acceptance harness to try a second launch during the handoff.
        std::thread::sleep(Duration::from_secs(1));
        return Ok(true);
    }
    if mode == "--apply-update" {
        if args.len() != 2 {
            return Err("Invalid update helper arguments".into());
        }
        let path = PathBuf::from(&args[1]);
        if let Err(error) = apply(&path) {
            if let Some(dir) = path.parent().and_then(|p| p.canonicalize().ok())
                && stage::root()
                    .and_then(|p| p.canonicalize().map_err(|e| e.to_string()))
                    .is_ok_and(|root| dir.parent() == Some(root.as_path()))
            {
                let _ = fs::write(dir.join("helper.error"), &error);
                if dir.join("helper.ready").exists() && !dir.join("rolled-back").exists() {
                    native::error_dialog(&format!("{error}\nRecovery files: {}", dir.display()));
                }
            }
            return Ok(true);
        }
        return Ok(true);
    }
    if mode == "--after-update" {
        if args.len() != 2 {
            return Err("Invalid update restart arguments".into());
        }
        let path = PathBuf::from(&args[1]);
        let job = Job::load(&path)?;
        let current = std::env::current_exe()
            .and_then(|p| p.canonicalize())
            .map_err(|e| e.to_string())?;
        if job.target != current
            || job.stage.release.manifest.version.to_string() != env!("CARGO_PKG_VERSION")
        {
            return Err("Update restart does not match the installed application".into());
        }
        stage::verify(&current, &job.stage.release)?;
        INCOMING
            .set(job.stage.dir)
            .map_err(|_| "Update restart was already initialized")?;
    }
    Ok(!incoming() && native::updating()?)
}
pub fn incoming() -> bool {
    INCOMING.get().is_some()
}
pub fn ready() -> Result<(), String> {
    if let Some(dir) = INCOMING.get() {
        marker(&dir.join("app.ready"), &std::process::id().to_string())?;
    }
    Ok(())
}
fn await_ready(child: &mut Child, dir: &Path) -> Result<(), String> {
    for _ in 0..300 {
        if fs::read_to_string(dir.join("app.ready")).is_ok_and(|s| s == child.id().to_string()) {
            return Ok(());
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Err(format!(
                "The new version exited before startup completed ({status})"
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("The new version did not finish starting".into())
}
fn apply(path: &Path) -> Result<(), String> {
    let mut guard = Some(native::UpdateLock::acquire()?);
    let job = Job::load(path)?;
    let parent = job.parent()?;
    if job.stage.release.manifest.version <= native::version(&job.target)? {
        return Err("Update must be newer than the installed version".into());
    }
    let helper = std::env::current_exe().map_err(|e| e.to_string())?;
    if stage::hash(&helper)?.1 != job.old_hash {
        return Err("Update helper does not match the running application".into());
    }
    stage::verify(&job.candidate(), &job.stage.release)?;
    marker(&job.stage.dir.join("helper.ready"), &job.pid.to_string())?;
    parent.wait()?;
    install(&job, &mut guard)
}
fn install(job: &Job, guard: &mut Option<native::UpdateLock>) -> Result<(), String> {
    if stage::hash(&job.target)?.1 != job.old_hash {
        return Err("The old executable changed; update cancelled".into());
    }
    let snapshot = match database::backup(&job.stage.dir, job.stage.release.manifest.schema) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            drop(guard.take());
            let _ = launch(&job.target).spawn();
            return Err(format!("Could not back up history: {error}"));
        }
    };
    let mut child = None;
    let attempt = (|| {
        stage::verify(&job.candidate(), &job.stage.release)?;
        native::replace(&job.target, &job.candidate(), &job.backup())?;
        child = Some(
            launch(&job.target)
                .arg("--after-update")
                .arg(job.path())
                .spawn()
                .map_err(|e| e.to_string())?,
        );
        await_ready(child.as_mut().unwrap(), &job.stage.dir)
    })();
    match attempt {
        Ok(()) => {
            super::save_result(&format!(
                "Updated to Clipdeck {}",
                job.stage.release.manifest.version
            ));
            let _ = fs::remove_file(job.stage.dir.join("download.exe"));
            Ok(())
        }
        Err(error) => {
            if let Some(mut child) = child {
                let _ = child.kill();
                let _ = child.wait();
                if let Some(snapshot) = snapshot {
                    database::restore(&snapshot, &job.nonce).map_err(|e| {
                        format!(
                            "{error}. History recovery needs attention: {e}. Backup: {}",
                            job.stage.dir.display()
                        )
                    })?;
                }
            }
            if job.backup().exists() {
                if stage::hash(&job.backup())?.1 != job.old_hash {
                    return Err(
                        "Previous executable backup changed; automatic recovery stopped".into(),
                    );
                }
                if job.target.exists() {
                    native::replace(
                        &job.target,
                        &job.backup(),
                        &job.target
                            .with_file_name(format!(".clipdeck-failed-{}.exe", job.nonce)),
                    )?;
                } else {
                    fs::rename(job.backup(), &job.target).map_err(|e| e.to_string())?;
                }
            }
            if stage::hash(&job.target)?.1 != job.old_hash {
                return Err(
                    "Previous executable could not be restored. Recovery files are preserved."
                        .into(),
                );
            }
            super::save_result(&format!(
                "Update failed; the previous version was restored. {error}"
            ));
            drop(guard.take());
            launch(&job.target)
                .arg("--update-recovered")
                .spawn()
                .map_err(|e| e.to_string())?;
            fs::write(job.stage.dir.join("rolled-back"), &error).map_err(|e| e.to_string())?;
            Err(error)
        }
    }
}
