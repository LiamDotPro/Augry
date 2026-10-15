//! Stopping kills the child, whether it is a fake or a real process.

use std::sync::atomic::Ordering;

use super::fakes::*;
use super::*;

#[test]
fn stop_kills_a_child_that_is_still_running() {
    let process = FakeProcess::new(Engine::Docker, vec![]);
    let (_seen, emit) = recorder();
    let (_delays, pause) = recording_pause();
    let sup = Supervisor::start(process.clone(), vec![], emit, pause).unwrap();
    wait_until("the first spawn", || {
        process.spawns.load(Ordering::SeqCst) == 1
    });
    sup.stop();
    assert!(process.kills.load(Ordering::SeqCst) >= 1);
    assert_eq!(process.spawns.load(Ordering::SeqCst), 1);
}

#[test]
fn dropping_the_supervisor_kills_the_child_too() {
    let process = FakeProcess::new(Engine::Docker, vec![]);
    let (_seen, emit) = recorder();
    let (_delays, pause) = recording_pause();
    let sup = Supervisor::start(process.clone(), vec![], emit, pause).unwrap();
    wait_until("the first spawn", || {
        process.spawns.load(Ordering::SeqCst) == 1
    });
    drop(sup);
    assert!(process.kills.load(Ordering::SeqCst) >= 1);
}

#[cfg(unix)]
mod real_child {
    use std::io::{BufRead, BufReader};
    use std::process::{Child, ChildStdout, Command, Stdio};

    use super::*;

    /// A real `sleep 1000` that never prints a line. It stands in for
    /// `docker events`, which also blocks for ever.
    struct SleepProcess {
        pid: Arc<Mutex<Option<u32>>>,
    }

    struct SleepLines {
        out: BufReader<ChildStdout>,
        child: Arc<Mutex<Child>>,
    }

    struct SleepKill {
        child: Arc<Mutex<Child>>,
    }

    impl Kill for SleepKill {
        fn kill(&self) {
            let mut child = lock(&self.child);
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    impl Lines for SleepLines {
        fn next_line(&mut self) -> Option<String> {
            let mut line = String::new();
            match self.out.read_line(&mut line) {
                Ok(0) | Err(_) => None,
                Ok(_) => Some(line.trim_end().to_string()),
            }
        }

        fn killer(&self) -> Arc<dyn Kill> {
            Arc::new(SleepKill {
                child: self.child.clone(),
            })
        }
    }

    impl EngineProcess for SleepProcess {
        fn engine(&self) -> Engine {
            Engine::Docker
        }

        fn spawn(&self) -> io::Result<Box<dyn Lines>> {
            let mut child = Command::new("sleep")
                .arg("1000")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()?;
            let out = BufReader::new(child.stdout.take().expect("stdout is piped"));
            *lock(&self.pid) = Some(child.id());
            Ok(Box::new(SleepLines {
                out,
                child: Arc::new(Mutex::new(child)),
            }))
        }
    }

    /// True if a process with this id exists (signal 0 only checks).
    fn alive(pid: u32) -> bool {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[test]
    fn stop_kills_and_reaps_a_real_long_running_child() {
        let pid = Arc::new(Mutex::new(None));
        let process = Arc::new(SleepProcess { pid: pid.clone() });
        let (_seen, emit) = recorder();
        let (_delays, pause) = recording_pause();
        let sup = Supervisor::start(process, vec![], emit, pause).unwrap();
        wait_until("the child to start", || lock(&pid).is_some());
        let pid = lock(&pid).unwrap();
        assert!(alive(pid), "sleep should be running before stop");
        let started = std::time::Instant::now();
        sup.stop();
        assert!(started.elapsed() < Duration::from_secs(5));
        // `wait` reaped it, so the id is gone and not a zombie.
        assert!(!alive(pid), "sleep should be gone after stop");
    }
}
