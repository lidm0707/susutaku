use std::collections::HashMap;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const TICK: Duration = Duration::from_millis(250);
pub const THREAD_NAME: &str = "schedule_task";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Schedule {
    /// Run every `secs` seconds after registration.
    Interval(u64),
    /// Run once at the given unix epoch seconds.
    Once(u64),
}

type Job = Box<dyn FnOnce() + Send>;

struct Register {
    schedule: Schedule,
    job: Job,
}

struct Entry {
    schedule: Schedule,
    next_due: SystemTime,
    job: Option<Job>,
}

enum Msg {
    Register(Register, Sender<u64>),
    Cancel(u64, Sender<bool>),
}

pub struct Scheduler {
    tx: Sender<Msg>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<Msg>();
        thread::Builder::new()
            .name(THREAD_NAME.into())
            .spawn(move || run(rx))
            .expect(THREAD_NAME);
        Self { tx }
    }

    pub fn register(&self, _name: &str, schedule: Schedule, job: Job) -> u64 {
        let (ack_tx, ack_rx) = mpsc::channel();
        self.tx
            .send(Msg::Register(Register { schedule, job }, ack_tx))
            .expect(THREAD_NAME);
        ack_rx.recv().expect(THREAD_NAME)
    }

    pub fn cancel(&self, id: u64) -> bool {
        let (ack_tx, ack_rx) = mpsc::channel();
        self.tx.send(Msg::Cancel(id, ack_tx)).expect(THREAD_NAME);
        ack_rx.recv().expect(THREAD_NAME)
    }
}

fn run(rx: mpsc::Receiver<Msg>) {
    let mut next_id: u64 = 0;
    let mut entries: HashMap<u64, Entry> = HashMap::new();
    loop {
        let now = SystemTime::now();
        let due: Vec<u64> = entries
            .iter()
            .filter(|(_, e)| e.next_due <= now && e.job.is_some())
            .map(|(id, _)| *id)
            .collect();
        for id in due {
            let entry = entries.get_mut(&id).expect("id collected from entries");
            if let Some(job) = entry.job.take() {
                job();
            }
            match entry.schedule {
                Schedule::Interval(secs) => {
                    entry.next_due = SystemTime::now() + Duration::from_secs(secs);
                }
                Schedule::Once(_) => {
                    entries.remove(&id);
                }
            }
        }
        match rx.recv_timeout(TICK) {
            Ok(Msg::Register(reg, ack)) => {
                let next_due = match reg.schedule {
                    Schedule::Interval(secs) => SystemTime::now() + Duration::from_secs(secs),
                    Schedule::Once(at) => UNIX_EPOCH + Duration::from_secs(at),
                };
                let id = next_id;
                next_id += 1;
                entries.insert(
                    id,
                    Entry {
                        schedule: reg.schedule,
                        next_due,
                        job: Some(reg.job),
                    },
                );
                let _ = ack.send(id);
            }
            Ok(Msg::Cancel(id, ack)) => {
                let _ = ack.send(entries.remove(&id).is_some());
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}
