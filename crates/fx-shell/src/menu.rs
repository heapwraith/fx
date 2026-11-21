//! Shell context menu (task T1.16). Platform-neutral part: the request, the
//! outcome, and a dispatcher with a time limit while the menu is built.
//!
//! Building the menu runs third-party shell extensions on a COM STA worker
//! thread. If the menu is not ready within the limit (an extension hangs),
//! the worker is abandoned and the caller gets [`MenuOutcome::Unavailable`];
//! the next menu uses a fresh worker. Once the menu is on screen there is no
//! limit: the user is choosing.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

/// How long building a menu may take before it is given up.
pub const MENU_PREPARE_LIMIT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
pub struct MenuRequest {
    /// Items the menu is for; empty for the menu of `folder` itself (right
    /// click on the empty part of the list).
    pub items: Vec<PathBuf>,
    pub folder: PathBuf,
    /// fx's own items, shown first.
    pub fx_items: Vec<String>,
    /// Window that owns dialogs the shell opens (Properties...).
    pub owner: isize,
    /// When the user asked (click time), for measuring.
    pub started: Instant,
    /// Build the menu without showing it (loads COM and the common shell
    /// extensions ahead of the first real menu).
    pub warm_up: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuOutcome {
    Cancelled,
    /// One of fx's own items (index into `fx_items`).
    Fx(usize),
    /// A shell command fx performs itself, so it goes through its queue and
    /// undo: "delete", "rename", "copy", "cut", "paste".
    Verb(String),
    /// A command the shell ran (open, properties, send to, extensions...).
    Shell,
    Unavailable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuReply {
    pub outcome: MenuOutcome,
    /// Click to menu on screen.
    pub ready_after: Option<Duration>,
    /// Of that, the time spent building the menu on the worker (the rest is
    /// waiting for the previous menu to close).
    pub built_in: Option<Duration>,
}

/// Shows a menu on the worker thread; calls `ready` right before showing it.
pub type MenuShower = Arc<dyn Fn(&MenuRequest, &dyn Fn()) -> MenuOutcome + Send + Sync>;

enum Step {
    Received(Instant),
    Ready(Instant),
    Done(MenuOutcome),
}

struct Worker {
    tx: Sender<(MenuRequest, Sender<Step>)>,
}

fn spawn_worker(show: &MenuShower, init: fn()) -> Option<Worker> {
    let (tx, rx) = channel::<(MenuRequest, Sender<Step>)>();
    let show = show.clone();
    std::thread::Builder::new()
        .name("fx-menu-sta".into())
        .spawn(move || {
            init();
            for (req, steps) in rx {
                let _ = steps.send(Step::Received(Instant::now()));
                let ready_steps = steps.clone();
                let ready = move || {
                    let _ = ready_steps.send(Step::Ready(Instant::now()));
                };
                let outcome = show(&req, &ready);
                let _ = steps.send(Step::Done(outcome));
            }
        })
        .ok()?;
    Some(Worker { tx })
}

/// One menu at a time, on a worker that is replaced when it hangs.
#[derive(Debug, Clone)]
pub struct MenuService {
    tx: Sender<(MenuRequest, Sender<MenuReply>)>,
}

impl MenuService {
    pub fn with_shower(show: MenuShower, init: fn(), limit: Duration) -> std::io::Result<Self> {
        let (tx, rx) = channel::<(MenuRequest, Sender<MenuReply>)>();
        std::thread::Builder::new()
            .name("fx-menu".into())
            .spawn(move || dispatch(&rx, &show, init, limit))?;
        Ok(Self { tx })
    }

    /// Asks for a menu; the reply arrives once the user chose (or it failed).
    pub fn show(&self, request: MenuRequest) -> Receiver<MenuReply> {
        let (tx, rx) = channel();
        let _ = self.tx.send((request, tx));
        rx
    }
}

fn dispatch(
    rx: &Receiver<(MenuRequest, Sender<MenuReply>)>,
    show: &MenuShower,
    init: fn(),
    limit: Duration,
) {
    let mut worker = spawn_worker(show, init);
    for (req, reply) in rx {
        let started = req.started;
        let (steps_tx, steps) = channel();
        let sent = worker
            .as_ref()
            .is_some_and(|w| w.tx.send((req, steps_tx)).is_ok());
        let answer = if !sent {
            worker = spawn_worker(show, init);
            MenuReply {
                outcome: MenuOutcome::Unavailable("menu indisponível".into()),
                ready_after: None,
                built_in: None,
            }
        } else {
            // The worker may still be showing the previous menu: the limit
            // counts from when it takes this request.
            let received = match steps.recv() {
                Ok(Step::Received(at)) => at,
                _ => Instant::now(),
            };
            match steps.recv_timeout(limit) {
                Ok(Step::Ready(at)) => {
                    let outcome = match steps.recv() {
                        Ok(Step::Done(o)) => o,
                        _ => MenuOutcome::Cancelled,
                    };
                    MenuReply {
                        outcome,
                        ready_after: Some(at.saturating_duration_since(started)),
                        built_in: Some(at.saturating_duration_since(received)),
                    }
                }
                Ok(Step::Done(outcome)) => MenuReply {
                    outcome,
                    ready_after: None,
                    built_in: None,
                },
                Ok(Step::Received(_)) => MenuReply {
                    outcome: MenuOutcome::Cancelled,
                    ready_after: None,
                    built_in: None,
                },
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {
                    // Stuck in a shell extension: leave that thread behind.
                    worker = spawn_worker(show, init);
                    MenuReply {
                        outcome: MenuOutcome::Unavailable(
                            "o menu demorou demais (uma extensão do shell travou)".into(),
                        ),
                        ready_after: None,
                        built_in: None,
                    }
                }
            }
        };
        let _ = reply.send(answer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(name: &str) -> MenuRequest {
        MenuRequest {
            items: vec![PathBuf::from(name)],
            folder: PathBuf::from(r"C:\x"),
            fx_items: Vec::new(),
            owner: 0,
            started: Instant::now(),
            warm_up: false,
        }
    }

    fn no_init() {}

    #[test]
    fn a_hung_extension_makes_the_menu_unavailable_and_the_next_one_works() {
        let show: MenuShower = Arc::new(|req, ready| {
            if req.items[0] == std::path::Path::new("hang") {
                loop {
                    std::thread::sleep(Duration::from_secs(60));
                }
            }
            ready();
            MenuOutcome::Fx(0)
        });
        let s = MenuService::with_shower(show, no_init, Duration::from_millis(200)).unwrap();
        let started = Instant::now();
        let hung = s
            .show(request("hang"))
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert!(matches!(hung.outcome, MenuOutcome::Unavailable(_)));
        assert!(started.elapsed() < Duration::from_secs(2));
        let ok = s
            .show(request("a.txt"))
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_eq!(ok.outcome, MenuOutcome::Fx(0));
        assert!(ok.ready_after.is_some());
    }

    #[test]
    fn choosing_takes_as_long_as_the_user_wants() {
        let show: MenuShower = Arc::new(|_, ready| {
            ready();
            std::thread::sleep(Duration::from_millis(400)); // the user reads the menu
            MenuOutcome::Verb("rename".into())
        });
        let s = MenuService::with_shower(show, no_init, Duration::from_millis(100)).unwrap();
        let r = s
            .show(request("a"))
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_eq!(r.outcome, MenuOutcome::Verb("rename".into()));
    }
}
