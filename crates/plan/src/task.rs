pub type TaskId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Todo,
    InProgress,
    Done,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: TaskId,
    pub title: String,
    pub status: Status,
    pub priority: Priority,
    pub depends_on: Vec<TaskId>,
}

impl Task {
    pub fn new(id: TaskId, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            status: Status::Todo,
            priority: Priority::Normal,
            depends_on: Vec::new(),
        }
    }

    pub fn is_runnable(&self, done: &[TaskId]) -> bool {
        self.status == Status::Todo && self.depends_on.iter().all(|d| done.contains(d))
    }
}

#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub name: String,
    tasks: Vec<Task>,
    next_id: TaskId,
}

impl Plan {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            tasks: Vec::new(),
            next_id: 1,
        }
    }

    pub fn add(&mut self, title: impl Into<String>) -> TaskId {
        let id = self.next_id;
        self.next_id += 1;
        self.tasks.push(Task::new(id, title));
        id
    }

    pub fn add_with(&mut self, build: impl FnOnce(TaskId) -> Task) -> TaskId {
        let id = self.next_id;
        self.next_id += 1;
        self.tasks.push(build(id));
        id
    }

    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    pub fn set_status(&mut self, id: TaskId, status: Status) -> bool {
        for t in &mut self.tasks {
            if t.id == id {
                t.status = status;
                return true;
            }
        }
        false
    }

    pub fn done_ids(&self) -> Vec<TaskId> {
        self.tasks
            .iter()
            .filter(|t| t.status == Status::Done)
            .map(|t| t.id)
            .collect()
    }

    pub fn runnable(&self) -> Vec<TaskId> {
        let done = self.done_ids();
        self.tasks
            .iter()
            .filter(|t| t.is_runnable(&done))
            .map(|t| t.id)
            .collect()
    }

    pub fn progress(&self) -> f64 {
        if self.tasks.is_empty() {
            return 0.0;
        }
        let done = self
            .tasks
            .iter()
            .filter(|t| t.status == Status::Done)
            .count();
        done as f64 / self.tasks.len() as f64
    }

    pub fn by_priority(&self) -> Vec<TaskId> {
        let mut ids: Vec<TaskId> = self.tasks.iter().map(|t| t.id).collect();
        ids.sort_by_key(|id| {
            let t = self.tasks.iter().find(|t| t.id == *id);
            std::cmp::Reverse(t.map(|t| t.priority).unwrap_or(Priority::Low))
        });
        ids
    }
}
