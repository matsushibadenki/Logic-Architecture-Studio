use std::{
    fs::{self, File, OpenOptions},
    io,
    os::{fd::AsRawFd, unix::process::CommandExt},
    path::{Path, PathBuf},
    process::Command,
};

pub(crate) struct ToolCgroup(PathBuf);

impl ToolCgroup {
    pub(crate) fn configured() -> Result<Option<Self>, String> {
        std::env::var_os("LAS_EDA_CGROUP")
            .map(|parent| {
                Self::create(Path::new(&parent), 4 * 1024 * 1024 * 1024, 200_000)
                    .map_err(|error| format!("Configure EDA cgroup: {error}"))
            })
            .transpose()
    }

    fn create(parent: &Path, memory_bytes: u64, cpu_micros: u64) -> io::Result<Self> {
        let parent = parent.canonicalize()?;
        if !parent.join("cgroup.controllers").is_file() {
            return Err(io::Error::other("Expected a delegated cgroup v2 directory"));
        }
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).map_err(|error| io::Error::other(error.to_string()))?;
        let path = parent.join(format!("las-eda-{:032x}", u128::from_ne_bytes(nonce)));
        fs::create_dir(&path)?;
        let group = Self(path);
        for (name, value) in [
            ("cpu.max", format!("{cpu_micros} 100000")),
            ("memory.max", memory_bytes.to_string()),
            ("memory.swap.max", "0".into()),
            ("memory.oom.group", "1".into()),
        ] {
            group.write(name, value.as_bytes())?;
        }
        // Validate cleanup permissions before admitting any child.
        group.kill()?;
        Ok(group)
    }

    fn write(&self, name: &str, value: &[u8]) -> io::Result<()> {
        use std::io::Write;
        OpenOptions::new()
            .write(true)
            .open(self.0.join(name))?
            .write_all(value)
    }

    pub(crate) fn attach(&self, command: &mut Command) -> io::Result<()> {
        let procs: File = OpenOptions::new()
            .write(true)
            .open(self.0.join("cgroup.procs"))?;
        // Writing 0 moves the calling child before it can exec or spawn descendants.
        unsafe {
            command.pre_exec(move || {
                if libc::write(procs.as_raw_fd(), b"0".as_ptr().cast(), 1) != 1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Ok(())
    }

    pub(crate) fn check(&self, cpu_budget_micros: u64) -> Result<(), String> {
        let read = |file, key| {
            self.counter(file, key)
                .map_err(|error| format!("Monitor EDA cgroup: {error}"))
        };
        if read("memory.events", "oom")? > 0 {
            return Err("Tool cgroup reached its kernel memory limit".into());
        }
        if read("cpu.stat", "usage_usec")? >= cpu_budget_micros {
            return Err("Tool cgroup exhausted its cumulative CPU budget".into());
        }
        Ok(())
    }

    fn counter(&self, file: &str, key: &str) -> io::Result<u64> {
        fs::read_to_string(self.0.join(file))?
            .lines()
            .filter_map(|line| line.split_once(' '))
            .find(|(name, _)| *name == key)
            .and_then(|(_, value)| value.trim().parse().ok())
            .ok_or_else(|| io::Error::other(format!("Missing cgroup counter {key}")))
    }

    pub(crate) fn kill(&self) -> io::Result<()> {
        self.write("cgroup.kill", b"1")
    }
}

impl Drop for ToolCgroup {
    fn drop(&mut self) {
        if let Err(error) = self.kill().and_then(|()| {
            for _ in 0..100 {
                match fs::remove_dir(&self.0) {
                    Ok(()) => return Ok(()),
                    Err(error) if error.raw_os_error() == Some(libc::EBUSY) => {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(error) => return Err(error),
                }
            }
            Err(io::Error::other("EDA cgroup remains populated"))
        }) {
            eprintln!("EDA cgroup cleanup failed: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Request, Service};
    use serde_json::json;
    use std::time::{Duration, Instant};

    #[test]
    #[ignore = "Requires delegated Linux cgroup v2, Python 3, Icarus Verilog and Yosys"]
    fn kernel_group_limits_and_eda_rerun() {
        let parent = PathBuf::from(std::env::var_os("LAS_EDA_CGROUP").unwrap());
        let wait = |predicate: &mut dyn FnMut() -> bool| {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !predicate() {
                assert!(
                    Instant::now() < deadline,
                    "Timed out waiting for cgroup event"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        };

        let cpu = ToolCgroup::create(&parent, 64 * 1024 * 1024, 10_000).unwrap();
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "(while :; do :; done) & wait"]);
        cpu.attach(&mut command).unwrap();
        let mut child = command.spawn().unwrap();
        wait(&mut || cpu.counter("cpu.stat", "nr_throttled").unwrap() > 0);
        wait(&mut || cpu.check(50_000).is_err());
        cpu.kill().unwrap();
        assert!(!child.wait().unwrap().success());
        wait(&mut || cpu.counter("cgroup.events", "populated").unwrap() == 0);
        drop(cpu);

        let memory = ToolCgroup::create(&parent, 48 * 1024 * 1024, 200_000).unwrap();
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            r#"
            sleep 60 &
            python3 -c 'data = bytearray(96 * 1024 * 1024)'
            wait
        "#,
        ]);
        memory.attach(&mut command).unwrap();
        let mut child = command.spawn().unwrap();
        wait(&mut || memory.counter("memory.events", "oom_kill").unwrap() > 0);
        assert!(memory.check(u64::MAX).unwrap_err().contains("memory limit"));
        wait(&mut || memory.counter("cgroup.events", "populated").unwrap() == 0);
        assert!(!child.wait().unwrap().success());
        drop(memory);

        let root = std::env::temp_dir().join(format!("las-cgroup-rerun-{}", std::process::id()));
        let service = Service::open(&root).unwrap();
        for (method, args) in [
            (
                "verify.rtl",
                json!({"expected_revision":0,"engine":"icarus"}),
            ),
            ("synthesis.run", json!({"expected_revision":0})),
        ] {
            let result = service
                .execute(Request {
                    method: method.into(),
                    args,
                    actor: "test".into(),
                })
                .unwrap();
            assert_eq!(result["success"], true, "{result}");
        }
        drop(service);
        fs::remove_dir_all(root).unwrap();
    }
}
