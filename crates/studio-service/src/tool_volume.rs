//! A capacity-limited filesystem for one macOS tool stage.
use std::{
    ffi::CString,
    fs, io,
    os::unix::{ffi::OsStrExt, fs::DirBuilderExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub(crate) struct ToolVolume {
    directory: PathBuf,
    pub(crate) work: PathBuf,
    mounted: bool,
}

impl ToolVolume {
    pub(crate) fn open(source: &Path) -> Result<Self, String> {
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).map_err(|error| error.to_string())?;
        let directory =
            std::env::temp_dir().join(format!("las-tool-{:032x}", u128::from_ne_bytes(nonce)));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .map_err(|error| error.to_string())?;
        let mount = directory.join("volume");
        let work = mount.join("run");
        let mut volume = Self {
            directory,
            work,
            mounted: false,
        };
        fs::create_dir(&mount).map_err(|error| error.to_string())?;
        let image = volume.directory.join("quota.sparseimage");
        hdiutil(
            Command::new("/usr/bin/hdiutil")
                .args([
                    "create",
                    "-quiet",
                    "-size",
                    "256m",
                    "-fs",
                    "Case-sensitive HFS+",
                    "-type",
                    "SPARSE",
                ])
                .arg(&image),
        )?;
        hdiutil(
            Command::new("/usr/bin/hdiutil")
                .args(["attach", "-nobrowse", "-owners", "off", "-mountpoint"])
                .arg(&mount)
                .arg(&image),
        )?;
        volume.mounted = true;
        fs::create_dir(&volume.work).map_err(|error| error.to_string())?;
        copy_files(source, &volume.work).map_err(|error| format!("Stage tool inputs: {error}"))?;
        Ok(volume)
    }

    pub(crate) fn publish(&self, destination: &Path) -> Result<(), String> {
        copy_files(&self.work, destination).map_err(|error| format!("Save tool artifacts: {error}"))
    }
}

impl Drop for ToolVolume {
    fn drop(&mut self) {
        if self.mounted {
            let result = hdiutil(
                Command::new("/usr/bin/hdiutil")
                    .args(["detach", "-force"])
                    .arg(self.directory.join("volume")),
            );
            if let Err(error) = result {
                eprintln!("EDA volume cleanup failed: {error}");
                return;
            }
        }
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn hdiutil(command: &mut Command) -> Result<(), String> {
    let output = command
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("Start EDA disk image utility: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "EDA disk image utility failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

fn copy_files(source: &Path, destination: &Path) -> io::Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        let kind = entry.file_type()?;
        let existing = fs::symlink_metadata(&target);
        if kind.is_dir() {
            match existing {
                Ok(metadata) if metadata.is_dir() => (),
                Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir(&target)?,
                _ => return Err(io::Error::other("Unsafe artifact destination")),
            }
            copy_files(&entry.path(), &target)?;
        } else {
            if !kind.is_file() {
                return Err(io::Error::other(
                    "EDA artifacts must be regular files or directories",
                ));
            }
            match existing {
                Ok(metadata) if metadata.is_file() => fs::remove_file(&target)?,
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                _ => return Err(io::Error::other("Unsafe artifact destination")),
            }
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

pub(crate) fn exhausted(directory: &Path) -> io::Result<bool> {
    let directory = CString::new(directory.as_os_str().as_bytes())?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // statvfs initializes stats on success.
    let stats = unsafe {
        if libc::statvfs(directory.as_ptr(), stats.as_mut_ptr()) != 0 {
            return Err(io::Error::last_os_error());
        }
        stats.assume_init()
    };
    Ok(u64::from(stats.f_bavail).saturating_mul(stats.f_frsize) <= 1024 * 1024)
}
