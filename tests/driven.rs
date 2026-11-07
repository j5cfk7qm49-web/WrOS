//! The real app in a pseudo-terminal (#13). vt100 reads what it draws, taria's
//! socket lets the test choose Settings rows by name, and insta keeps snapshots
//! of the screens. Build with `--features test-driver`; see docs/TESTING.md.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const BINARY: &str = env!("CARGO_BIN_EXE_grimoire");
const WAIT: Duration = Duration::from_secs(15);

/// A temporary home with the tutorial's demo book in it.
struct Home {
    dir: PathBuf,
}

impl Home {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("wros-driven-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("home")).unwrap();
        // taria only binds its socket in a folder no one else can open.
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let made = Command::new(BINARY)
            .args(["wros", "tutorial-demo"])
            .arg(dir.join("book"))
            .envs(Self::env(&dir))
            .output()
            .unwrap();
        assert!(made.status.success(), "{made:?}");
        Self { dir }
    }

    fn env(dir: &Path) -> Vec<(&'static str, PathBuf)> {
        let home = dir.join("home");
        vec![
            ("XDG_DATA_HOME", home.join("data")),
            ("XDG_CONFIG_HOME", home.join("config")),
            ("HOME", home),
        ]
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The app running in a pseudo-terminal of `width` × `height`.
struct App {
    child: Child,
    master: Arc<OwnedFd>,
    screen: Arc<Mutex<vt100::Parser>>,
    driver: Driver,
}

impl App {
    fn start(home: &Home, width: u16, height: u16) -> Self {
        let (mut master, mut slave) = (0, 0);
        let size = libc::winsize {
            ws_row: height,
            ws_col: width,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let opened = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                &size,
            )
        };
        assert_eq!(opened, 0, "openpty");
        let (master, slave) =
            unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
        let socket = home.dir.join("driver.sock");
        let mut command = Command::new(BINARY);
        command
            .arg(home.dir.join("book"))
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("TERM", "xterm-256color")
            .env("LANG", "C.UTF-8")
            .env("WROS_TEST_DRIVER_SOCKET", &socket)
            .envs(Home::env(&home.dir))
            .current_dir(home.dir.join("home"))
            .stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::from(slave));
        unsafe {
            command.pre_exec(|| {
                // Its own session, with the pseudo-terminal as its terminal.
                if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().unwrap();
        let master = Arc::new(master);
        let rows_cols = (
            std::num::NonZero::new(height).unwrap(),
            std::num::NonZero::new(width).unwrap(),
        );
        let screen = Arc::new(Mutex::new(vt100::Parser::new(rows_cols.0, rows_cols.1, 0)));
        {
            let (master, screen) = (Arc::clone(&master), Arc::clone(&screen));
            std::thread::spawn(move || {
                let mut buffer = [0u8; 65536];
                let mut tail = Vec::new();
                loop {
                    let read = unsafe {
                        libc::read(master.as_raw_fd(), buffer.as_mut_ptr().cast(), buffer.len())
                    };
                    if read <= 0 {
                        break;
                    }
                    let chunk = &buffer[..read as usize];
                    // The app asks where the cursor is at start; answer as a terminal would.
                    tail.extend_from_slice(chunk);
                    let asks = tail.windows(4).filter(|w| w == b"\x1b[6n").count();
                    for _ in 0..asks {
                        let reply = b"\x1b[1;1R";
                        unsafe {
                            libc::write(master.as_raw_fd(), reply.as_ptr().cast(), reply.len())
                        };
                    }
                    let keep = tail.len().saturating_sub(3);
                    tail.drain(..keep);
                    screen.lock().unwrap().process(chunk);
                }
            });
        }
        let driver = Driver::connect(&socket);
        let app = Self {
            child,
            master,
            screen,
            driver,
        };
        app.until("the book", |s, _| {
            s.contains("Brass Key") || s.contains("WrOS")
        });
        app
    }

    fn screen(&self) -> String {
        self.screen.lock().unwrap().screen().contents()
    }

    fn send(&self, bytes: &[u8]) {
        let written =
            unsafe { libc::write(self.master.as_raw_fd(), bytes.as_ptr().cast(), bytes.len()) };
        assert_eq!(written, bytes.len() as isize);
    }

    /// Waits until `ready` holds for the screen and the latest published tree.
    fn until(&self, what: &str, ready: impl Fn(&str, &serde_json::Value) -> bool) {
        let end = Instant::now() + WAIT;
        loop {
            let (screen, tree) = (self.screen(), self.driver.tree());
            if ready(&screen, &tree) {
                return;
            }
            assert!(
                Instant::now() < end,
                "waited for {what}\n{screen}\n{tree:#}"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Chooses the row labelled `label` on the open Settings page.
    fn choose(&mut self, label: &str) {
        self.until(label, |_, tree| row(tree, label).is_some());
        let id = row(&self.driver.tree(), label).unwrap();
        self.driver.act(&id);
    }

    fn activate(&mut self, id: &str) {
        self.until(id, |_, tree| node(tree, &|n| n["id"] == id).is_some());
        self.driver.act(id);
    }

    fn title(&self) -> String {
        node(&self.driver.tree(), &|n| n["id"] == "page")
            .and_then(|page| page["label"].as_str().map(str::to_string))
            .unwrap_or_default()
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The test's side of the taria socket: the newest tree the app published, and inputs.
struct Driver {
    writer: UnixStream,
    tree: Arc<Mutex<serde_json::Value>>,
    next: u64,
}

impl Driver {
    fn connect(path: &Path) -> Self {
        let end = Instant::now() + WAIT;
        let stream = loop {
            match UnixStream::connect(path) {
                Ok(stream) => break stream,
                Err(_) if Instant::now() < end => std::thread::sleep(Duration::from_millis(50)),
                Err(error) => panic!("the test driver socket never opened: {error}"),
            }
        };
        let tree = Arc::new(Mutex::new(serde_json::Value::Null));
        let hello = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        {
            let (reader, tree, hello) = (
                stream.try_clone().unwrap(),
                Arc::clone(&tree),
                Arc::clone(&hello),
            );
            std::thread::spawn(move || {
                for line in BufReader::new(reader).lines().map_while(Result::ok) {
                    let Ok(message) = serde_json::from_str::<serde_json::Value>(&line) else {
                        continue;
                    };
                    match message["type"].as_str() {
                        Some("hello") => {
                            *hello.0.lock().unwrap() = true;
                            hello.1.notify_all();
                        }
                        Some("snapshot") => *tree.lock().unwrap() = message,
                        _ => {}
                    }
                }
            });
        }
        // Input written before the app's hello is thrown away.
        let (said, wake) = &*hello;
        let said = wake
            .wait_timeout_while(said.lock().unwrap(), WAIT, |said| !*said)
            .unwrap();
        assert!(*said.0, "no hello from the app");
        Self {
            writer: stream,
            tree,
            next: 1,
        }
    }

    fn tree(&self) -> serde_json::Value {
        self.tree.lock().unwrap().clone()
    }

    fn act(&mut self, node: &str) {
        let message = serde_json::json!({
            "type": "input",
            "id": self.next,
            "input": {"kind": "act", "node": node, "action": "activate"},
        });
        self.next += 1;
        writeln!(self.writer, "{message}").unwrap();
    }
}

/// The first node in `tree` that `wanted` accepts.
fn node<'t>(
    tree: &'t serde_json::Value,
    wanted: &dyn Fn(&serde_json::Value) -> bool,
) -> Option<&'t serde_json::Value> {
    if wanted(tree) {
        return Some(tree);
    }
    match tree {
        serde_json::Value::Array(items) => items.iter().find_map(|item| node(item, wanted)),
        serde_json::Value::Object(fields) => fields.values().find_map(|value| node(value, wanted)),
        _ => None,
    }
}

/// The id of the choosable row whose label starts with `label`.
fn row(tree: &serde_json::Value, label: &str) -> Option<String> {
    node(tree, &|n| {
        n["role"] == "list_item"
            && n["label"].as_str().is_some_and(|l| l.starts_with(label))
            && n["actions"].as_array().is_some_and(|a| !a.is_empty())
    })
    .and_then(|n| n["id"].as_str().map(str::to_string))
}

/// The screen as text, trimmed on the right, for a snapshot.
fn plain(screen: &str) -> String {
    screen
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn settings_pages_are_reached_by_name_and_drawn_as_their_snapshots() {
    let home = Home::new("settings");
    let mut app = App::start(&home, 100, 30);
    app.activate("open-settings");
    app.until("Settings", |_, tree| {
        node(tree, &|n| n["id"] == "page").is_some()
    });
    insta::assert_snapshot!("settings_top", plain(&app.screen()));

    for (group, page) in [
        ("Backups and accounts", "Google Drive"),
        ("Updates and recovery", "Recovery"),
    ] {
        app.choose(group);
        let opened = format!("Settings › {group}");
        app.until(&opened, |s, _| s.contains(&opened));
        app.choose(page);
        let want = format!("Settings › {group} › {page}");
        app.until(&want, |s, _| s.contains(&want));
        assert!(app.title().contains(&want), "{}", app.title());
        app.send(b"\x1b");
        app.until("back in the group", |s, _| !s.contains(&want));
        app.send(b"\x1b");
        app.until("back at the top", |_, tree| {
            node(tree, &|n| n["id"] == "page")
                .is_some_and(|p| p["label"].as_str().is_some_and(|l| !l.contains('›')))
        });
    }
    app.choose("Updates and recovery");
    app.until("the Updates and recovery page", |s, _| {
        s.contains("Settings › Updates and recovery")
    });
    insta::assert_snapshot!("updates_and_recovery", plain(&app.screen()));
}

#[test]
fn a_narrow_console_lists_every_system_page() {
    let home = Home::new("narrow");
    let mut app = App::start(&home, 80, 24);
    app.activate("open-settings");
    app.choose("System");
    app.until("System", |s, _| s.contains("Settings › System"));
    for page in [
        "Wi-Fi and networking",
        "Display",
        "Battery and power",
        "Date and time",
    ] {
        assert!(row(&app.driver.tree(), page).is_some(), "{page}");
    }
    assert!(app.screen().contains("Battery and power"));
}

#[test]
fn typed_words_reach_the_book_and_the_published_tree_carries_no_prose() {
    let home = Home::new("typing");
    let app = App::start(&home, 100, 30);
    // Ctrl-K finds the scene, as a person would.
    app.send(b"\x0bThe Brass Key\r");
    app.until("the scene", |s, _| s.contains("Brass Key"));
    app.send(b"Typed through the terminal. ");
    app.until("typed words", |s, _| {
        s.contains("Typed through the terminal.")
    });
    let tree = app.driver.tree().to_string();
    assert!(!tree.contains("Typed through the terminal"), "{tree}");
}
