use std::collections::VecDeque;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct SingleJob {
    pub id: Uuid,
    pub exec: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct JobSet {
    pub jobs: Vec<Job>,
}

#[derive(Clone, Debug)]
pub enum Job {
    SingleJob,
    JobSet,
}

#[derive(Clone, Debug)]
pub struct JobQueue {
    pub jobs: VecDeque<Job>,
    pub results: Vec<Uuid>,
}

fn main() {
    println!("This is a stub");
}
