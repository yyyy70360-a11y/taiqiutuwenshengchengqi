use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
};
use tokio::sync::Notify;

#[derive(Default)]
pub struct Control {
    cancelled: AtomicBool,
    notify: Notify,
}

impl Control {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    pub async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        self.notify.notified().await;
    }
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.notify.notify_one();
    }
}

type Registry = HashMap<String, (String, Arc<Control>)>;
fn registry() -> &'static Mutex<Registry> {
    static JOBS: OnceLock<Mutex<Registry>> = OnceLock::new();
    JOBS.get_or_init(Mutex::default)
}

pub struct Lease {
    pub id: String,
    pub control: Arc<Control>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut jobs) = registry().lock() {
            jobs.remove(&self.id);
        }
    }
}

pub fn acquire(kind: &str, id: Option<String>) -> Result<Lease, String> {
    let id = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    if id.is_empty()
        || id.len() > 80
        || !id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
    {
        return Err("任务编号无效".into());
    }
    let mut jobs = registry().lock().map_err(|_| "任务锁不可用")?;
    if jobs.contains_key(&id) || jobs.values().any(|(active, _)| active == kind) {
        return Err("同类任务正在运行，请等待完成或先取消".into());
    }
    let control = Arc::new(Control::default());
    jobs.insert(id.clone(), (kind.into(), control.clone()));
    Ok(Lease { id, control })
}

pub fn cancel(id: &str) -> Result<(), String> {
    let jobs = registry().lock().map_err(|_| "任务锁不可用")?;
    let (_, control) = jobs.get(id).ok_or("任务已完成或不存在")?;
    control.cancel();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_task_is_blocked_and_lease_releases_it() {
        let first = acquire("test-render", Some("test-first".into())).unwrap();
        assert!(acquire("test-render", Some("test-second".into())).is_err());
        cancel(&first.id).unwrap();
        assert!(first.control.is_cancelled());
        drop(first);
        assert!(acquire("test-render", Some("test-second".into())).is_ok());
    }
    #[test]
    fn cancellation_is_scoped_to_the_target_job() {
        let a = acquire("test-a", None).unwrap();
        let b = acquire("test-b", None).unwrap();
        cancel(&a.id).unwrap();
        assert!(a.control.is_cancelled());
        assert!(!b.control.is_cancelled());
    }
}
