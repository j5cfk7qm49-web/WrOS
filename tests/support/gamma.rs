//! A private two-output Wayland server, compiled only by colour tests.
use std::{
    fs,
    io::{BufRead, BufReader},
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

pub struct Server {
    child: Child,
    pub dir: PathBuf,
}
impl Server {
    pub fn start(mode: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "wros-gamma-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        let tests = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("os/tests");
        let xml = tests.join("wlr-gamma-control-unstable-v1.xml");
        for (operation, output) in [
            ("server-header", "gamma-server.h"),
            ("private-code", "gamma-code.c"),
        ] {
            assert!(
                Command::new("wayland-scanner")
                    .arg(operation)
                    .arg(&xml)
                    .arg(dir.join(output))
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let compile = Command::new("cc")
            .args(["-std=c11", "-Wall", "-Wextra", "-Werror"])
            .arg("-I")
            .arg(&dir)
            .arg(tests.join("wayland-gamma-server.c"))
            .arg(dir.join("gamma-code.c"))
            .args(["-lwayland-server", "-o"])
            .arg(dir.join("server"))
            .output()
            .unwrap();
        assert!(
            compile.status.success(),
            "{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let mut child = Command::new(dir.join("server"))
            .arg(mode)
            .arg(dir.join("curves.log"))
            .env("XDG_RUNTIME_DIR", &dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut ready = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        assert_eq!(ready, "ready\n");
        Self { child, dir }
    }
    pub fn log(&self) -> String {
        fs::read_to_string(self.dir.join("curves.log")).unwrap()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.dir);
    }
}
