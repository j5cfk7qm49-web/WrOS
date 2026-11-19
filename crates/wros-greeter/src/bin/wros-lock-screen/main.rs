//! `wros-lock-screen --user NAME`: the writing session's lock screen (#158).
//!
//! wros-lock's root worker starts it where it used to start vlock-main
//! (`openvt -c 8 -s -w -- wros-lock-screen --user NAME`), and it keeps vlock-main's contract:
//! it holds tty8 in VT_PROCESS mode and refuses every switch away, turns echo and SysRq off, and
//! exits 0 only once PAM's `vlock` stack has accepted the writer's password. Any other ending
//! (an error, a crash, a kill) is a failed lock, which closes the writing session safely.
//!
//! As root it only disables SysRq, opens tty8 and the display (taking DRM master and the VT),
//! then gives up root for good and runs as the writer: the theme, the keys and the password are
//! handled without privilege, and pam_unix checks the password through its setuid
//! `unix_chkpwd`, which only checks the caller's own. It draws the login screen's logo, clock
//! and form on a thread of its own: the keys and the password check never wait on the display,
//! so the right password unlocks even when drawing fails (after waking, say).

#[cfg(target_os = "linux")]
mod pam;

#[cfg(target_os = "linux")]
fn main() {
    // Never format a panic payload anywhere: it could hold typed text.
    std::panic::set_hook(Box::new(|_| {}));
    let code = std::panic::catch_unwind(linux::run).unwrap_or(1);
    std::process::exit(code);
}

#[cfg(not(target_os = "linux"))]
fn main() {
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
mod linux {
    use super::pam;
    use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
    use std::{
        ffi::{CStr, CString},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        time::Duration,
    };
    use wros_greeter::form::{Action, Form};
    use zeroize::Zeroizing;

    const TTY: &CStr = c"/dev/tty8";
    /// Where the unlock is proven to wros-lock's worker (its root-only runtime directory). The
    /// worker creates it empty; it is opened here while still root, and `unlocked` is written
    /// into it only after PAM accepted the password. Killing this program, which the writer's
    /// own programs can do, leaves it empty: the lock fails and the session closes.
    const PROOF: &str = "/run/wros-lock/unlocked";
    const VT_SETMODE: libc::c_ulong = 0x5602;
    const VT_RELDISP: libc::c_ulong = 0x5605;

    const CHECKING: &str = "Checking…";
    const WRONG: &str = "That password didn't work. Try again.";

    /// A fixed line for the journal; never typed text.
    fn log(message: &'static CStr) {
        unsafe {
            libc::syslog(
                libc::LOG_ERR,
                c"wros-lock-screen: %s".as_ptr(),
                message.as_ptr(),
            );
        }
    }

    pub fn run() -> i32 {
        match lock() {
            Ok(()) => 0,
            Err(reason) => {
                log(reason);
                1
            }
        }
    }

    struct Account {
        name: CString,
        uid: libc::uid_t,
        gid: libc::gid_t,
    }

    /// The writer named by `--user NAME`, the only argument: a local account other than root.
    fn account() -> Result<Account, &'static CStr> {
        let arguments: Vec<_> = std::env::args_os().skip(1).collect();
        let [flag, name] = arguments.as_slice() else {
            return Err(c"usage: wros-lock-screen --user NAME");
        };
        let name = name.to_str().filter(|_| flag == "--user");
        let Some(name) = name.filter(|name| {
            (1..=64).contains(&name.len())
                && name.starts_with(|c: char| c.is_ascii_alphabetic())
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_-.".contains(c))
        }) else {
            return Err(c"usage: wros-lock-screen --user NAME");
        };
        let name = CString::new(name).map_err(|_| c"invalid account name")?;
        let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut buffer = vec![0 as libc::c_char; 16384];
        let mut found = std::ptr::null_mut();
        let status = unsafe {
            libc::getpwnam_r(
                name.as_ptr(),
                entry.as_mut_ptr(),
                buffer.as_mut_ptr(),
                buffer.len(),
                &mut found,
            )
        };
        if status != 0 || found.is_null() {
            return Err(c"no such account");
        }
        let entry = unsafe { entry.assume_init() };
        if entry.pw_uid == 0 {
            return Err(c"refusing to lock for root");
        }
        Ok(Account {
            name,
            uid: entry.pw_uid,
            gid: entry.pw_gid,
        })
    }

    /// No core dump and no ptrace for a process that holds a password. Called again after the
    /// change of user, which resets the dumpable flag.
    fn harden() -> Result<(), &'static CStr> {
        let none = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &none) } != 0
            || unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) } != 0
        {
            return Err(c"process hardening failed");
        }
        Ok(())
    }

    /// Becomes the writer for good: groups, then group, then user, and root can't come back.
    fn become_writer(account: &Account) -> Result<(), &'static CStr> {
        let (mut ruid, mut euid, mut suid) = (0, 0, 0);
        let (mut rgid, mut egid, mut sgid) = (0, 0, 0);
        if unsafe { libc::initgroups(account.name.as_ptr(), account.gid) } != 0
            || unsafe { libc::setresgid(account.gid, account.gid, account.gid) } != 0
            || unsafe { libc::setresuid(account.uid, account.uid, account.uid) } != 0
            || unsafe { libc::getresuid(&mut ruid, &mut euid, &mut suid) } != 0
            || unsafe { libc::getresgid(&mut rgid, &mut egid, &mut sgid) } != 0
            || [ruid, euid, suid] != [account.uid; 3]
            || [rgid, egid, sgid] != [account.gid; 3]
            || unsafe { libc::setuid(0) } == 0
        {
            return Err(c"could not give up root");
        }
        Ok(())
    }

    #[repr(C)]
    #[derive(Default)]
    struct VtMode {
        mode: u8,
        wait: u8,
        release: i16,
        acquire: i16,
        force: i16,
    }

    /// Without a display (a text lock screen), holds the VT itself: VT_PROCESS mode on tty8,
    /// every release request refused, as vlock's `all` plugin does.
    fn hold_text_vt() -> Result<(), &'static CStr> {
        let release = Arc::new(AtomicBool::new(false));
        let acquire = Arc::new(AtomicBool::new(false));
        signal_hook::flag::register(libc::SIGUSR1, release.clone())
            .map_err(|_| c"VT signals unavailable")?;
        signal_hook::flag::register(libc::SIGUSR2, acquire.clone())
            .map_err(|_| c"VT signals unavailable")?;
        let mode = VtMode {
            mode: 1,
            release: libc::SIGUSR1 as i16,
            acquire: libc::SIGUSR2 as i16,
            ..VtMode::default()
        };
        if unsafe { libc::ioctl(libc::STDIN_FILENO, VT_SETMODE, &mode) } != 0 {
            return Err(c"could not hold tty8");
        }
        std::thread::spawn(move || {
            loop {
                if release.swap(false, Ordering::SeqCst) {
                    unsafe { libc::ioctl(libc::STDIN_FILENO, VT_RELDISP, 0) };
                }
                if acquire.swap(false, Ordering::SeqCst) {
                    unsafe { libc::ioctl(libc::STDIN_FILENO, VT_RELDISP, 2) };
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        });
        Ok(())
    }

    /// Standard input is tty8, as openvt starts it.
    fn on_tty8() -> bool {
        let name = unsafe { libc::ttyname(libc::STDIN_FILENO) };
        !name.is_null() && unsafe { CStr::from_ptr(name) } == TTY
    }

    /// What the drawing thread shows: never the password, nor even its length (the form shows
    /// a constant mark for any typed secret).
    struct View {
        typed: bool,
        notice: &'static str,
    }

    fn lock() -> Result<(), &'static CStr> {
        harden()?;
        if unsafe { libc::geteuid() } != 0 {
            return Err(c"must be started by the lock worker");
        }
        let account = account()?;
        if !on_tty8() {
            return Err(c"standard input is not tty8");
        }
        // As root: SysRq off (vlock's nosysrq; the worker puts it back after), then the
        // display and the VT.
        std::fs::write("/proc/sys/kernel/sysrq", "0").map_err(|_| c"could not disable SysRq")?;
        let mut proof = open_proof()?;
        let mut terminal = wros_graphics::init(true).map_err(|_| c"terminal setup failed")?;
        let graphical = terminal.backend_mut().hold_for_lock();
        if !graphical {
            hold_text_vt()?;
        }
        become_writer(&account)?;
        harden()?;
        // From here on, the writer.
        let palette = std::fs::read_to_string("/etc/wros/boot-theme.toml")
            .map(|text| wros_shell::grimoire_theme::from_config(&text))
            .unwrap_or_else(|_| wros_shell::grimoire_theme::default_theme());
        let palette = if graphical {
            palette
        } else {
            palette.for_console()
        };
        // Keys typed before the screen was up are dropped.
        unsafe { libc::tcflush(libc::STDIN_FILENO, libc::TCIFLUSH) };
        let (views, shown) = mpsc::channel::<View>();
        let user = account.name.to_string_lossy().into_owned();
        let painter = std::thread::spawn(move || paint(terminal, shown, &user, &palette));
        let unlocked = keys(&account, views).and_then(|()| {
            use std::io::Write;
            proof
                .write_all(b"unlocked\n")
                .map_err(|_| c"could not prove the unlock")
        });
        // The drawing thread ends once the sender is gone, putting the console back.
        let _ = painter.join();
        ratatui::restore();
        unlocked
    }

    /// The worker's empty, root-owned proof file, opened for writing without following links.
    fn open_proof() -> Result<std::fs::File, &'static CStr> {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        let proof = std::fs::OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(PROOF)
            .map_err(|_| c"no unlock proof from the lock worker")?;
        let info = proof
            .metadata()
            .map_err(|_| c"no unlock proof from the lock worker")?;
        if !info.is_file() || info.uid() != 0 || info.nlink() != 1 || info.len() != 0 {
            return Err(c"the unlock proof is not the worker's");
        }
        Ok(proof)
    }

    /// Draws the lock screen until the key loop ends. A drawing error is noted once and drawing
    /// carries on: the keys and the password check never depend on it.
    fn paint(
        mut terminal: ratatui::Terminal<wros_graphics::Screen<std::io::Stdout>>,
        shown: mpsc::Receiver<View>,
        user: &str,
        palette: &wros_shell::grimoire_theme::Theme,
    ) {
        let mut form = Form::locked(user);
        let mut view = View {
            typed: false,
            notice: "",
        };
        let mut drawn = None;
        let mut dirty = true;
        let mut failed = false;
        loop {
            match shown.recv_timeout(Duration::from_millis(100)) {
                Ok(next) => {
                    view = next;
                    dirty = true;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            let time = wros_greeter::local_time();
            if dirty || time != drawn || terminal.backend().vt_switch_pending() {
                form.password.set(if view.typed { "*" } else { "" });
                let result = terminal.draw(|frame| {
                    wros_greeter::render_lock(frame, &form, palette, time, view.notice)
                });
                if result.is_err() && !failed {
                    log(c"drawing failed; the password still unlocks");
                    failed = true;
                }
                drawn = time;
                dirty = false;
            }
        }
        // Dropping the terminal puts the console back before the worker switches to tty1.
        drop(terminal);
    }

    /// Reads keys until PAM accepts the password. Only this thread and the short-lived check
    /// ever hold it, in memory that is wiped when dropped.
    fn keys(account: &Account, views: mpsc::Sender<View>) -> Result<(), &'static CStr> {
        let mut form = Form::locked(&account.name.to_string_lossy());
        let mut checking: Option<mpsc::Receiver<bool>> = None;
        let mut notice = "";
        let _ = views.send(View {
            typed: false,
            notice,
        });
        loop {
            if let Some(answer) = &checking {
                match answer.try_recv() {
                    Ok(true) => return Ok(()),
                    Ok(false) | Err(mpsc::TryRecvError::Disconnected) => {
                        checking = None;
                        notice = WRONG;
                        let _ = views.send(View {
                            typed: !form.password.is_empty(),
                            notice,
                        });
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }
            if !event::poll(Duration::from_millis(100)).map_err(|_| c"keyboard read failed")? {
                continue;
            }
            let Event::Key(key) = event::read().map_err(|_| c"keyboard read failed")? else {
                continue;
            };
            // Keys typed while PAM checks are dropped.
            if key.kind != KeyEventKind::Press || checking.is_some() {
                continue;
            }
            if key.code == KeyCode::Esc {
                form.password.clear();
            } else if form.key(key) == Action::Submit && !form.password.is_empty() {
                let password = Zeroizing::new(form.password.text().to_owned());
                form.password.clear();
                let user = account.name.clone();
                let (answer, answered) = mpsc::channel();
                std::thread::spawn(move || {
                    let _ = answer.send(pam::authenticate(&user, &password, TTY));
                });
                checking = Some(answered);
                notice = CHECKING;
            }
            let _ = views.send(View {
                typed: !form.password.is_empty(),
                notice,
            });
        }
    }
}
