//! The **task** manager implementation.
//!
//! Background task tracking: long-running operations (export, AI jobs,
//! plugin installs) are spawned through this service so the host can show
//! progress and cancel them.
//!
//! # Thread model
//!
//! Every task runs on its own background thread. The service keeps a
//! [`JoinHandle`] per task so [`TaskService::join`] blocks until the thread
//! has *actually* finished — it never holds the service lock while blocking,
//! so a task body that calls back into the service (e.g.
//! `report_progress`) cannot deadlock. Dropping the service detaches
//! still-running tasks: their threads keep running to completion.
//!
//! # Cancellation
//!
//! Cancellation is cooperative and **sticky**. [`TaskService::cancel`] only
//! records the request; it never interrupts the running thread. Once a task
//! is cancelled, its final status is [`TaskStatus::Cancelled`] even if the
//! body runs to completion, and a task that already finished cannot be
//! cancelled (the call is a no-op).
//!
//! # Panics
//!
//! A panicking task body is caught with `catch_unwind` and reported as
//! [`TaskStatus::Failed`] carrying the panic message (`&str` and `String`
//! payloads are extracted verbatim; other payloads get a generic message).
//!
//! # Progress
//!
//! [`TaskService::report_progress`] clamps into `[0.0, 1.0]`: NaN is treated
//! as `0.0`, and ±infinity clamp to the nearest bound.
//!
//! # Cleanup
//!
//! Finished tasks stay tracked so callers can read their final status later,
//! but an unbounded number of them would grow the task map forever.
//! [`TaskService::prune_finished`] removes finished tasks explicitly, and
//! [`TaskService::spawn`] auto-prunes finished tasks once the map grows past
//! [`AUTO_PRUNE_THRESHOLD`], keeping the map bounded even without an
//! explicit call.

use std::any::Any;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread::JoinHandle;

use cordis::{Context, Inject, PluginHandle, Service, service_sync};
use kaleido_traits::services::task::{TaskId, TaskService, TaskStatus};
use kaleido_traits::services::{ServiceError, ServiceResult};

/// Auto-prune trigger: `spawn` sweeps finished tasks once the map already
/// holds at least this many entries, so the map stays bounded even if
/// callers never call [`TaskService::prune_finished`].
const AUTO_PRUNE_THRESHOLD: usize = 256;

/// Recovers the guarded value from a poisoned lock.
///
/// The data behind a poisoned lock is still valid — only the flag recording
/// the panic is set — so taking the inner value back keeps the service
/// operational. A poison can only occur if a panic happened while a lock was
/// held, which the implementation never does (task bodies run without
/// holding any lock); this is purely defensive.
fn recover<T>(poisoned: std::sync::PoisonError<T>) -> T {
    poisoned.into_inner()
}

/// Internal bookkeeping for a spawned task.
pub struct TaskEntry {
    pub name: String,
    /// Shared with the running thread; the task's lifecycle state.
    pub status: Arc<Mutex<TaskStatus>>,
    /// `(finished, condvar)`: `finished` flips to `true` once the thread has
    /// written its final status. `join` / `join_timeout` wait on it, and
    /// pruning uses it to distinguish "thread terminated" from "cancelled
    /// but the thread is still running".
    pub done: Arc<(Mutex<bool>, Condvar)>,
    /// The thread handle, taken by the first `join`.
    pub handle: Option<JoinHandle<()>>,
}

impl TaskEntry {
    fn is_finished(&self) -> bool {
        *self.done.0.lock().unwrap_or_else(recover)
    }
}

/// Default implementation of [`TaskService`].
pub struct TaskServiceImpl {
    // Kept for future event emission, matching the other manager services.
    ctx: Context,
    tasks: RwLock<HashMap<TaskId, TaskEntry>>,
    next_id: AtomicU64,
}

impl TaskServiceImpl {
    pub fn new(ctx: Context) -> Self {
        Self {
            ctx,
            tasks: RwLock::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    /// Runs the task body on the spawned thread.
    ///
    /// Sets the status to `Running(0.0)` first, then runs the body under
    /// `catch_unwind` so a panic becomes [`TaskStatus::Failed`] instead of
    /// aborting the thread or poisoning a lock. Finally the status is
    /// finalized — unless the task was cancelled, in which case the
    /// cancellation is sticky — and the `done` flag is raised so `join` /
    /// `join_timeout` / pruning can observe completion.
    fn run_task(
        status: &Arc<Mutex<TaskStatus>>,
        done: &Arc<(Mutex<bool>, Condvar)>,
        task: Box<dyn FnOnce() + Send + 'static>,
    ) {
        {
            let mut guard = status.lock().unwrap_or_else(recover);
            *guard = TaskStatus::Running(0.0);
        }

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(task));
        let final_status = match result {
            Ok(()) => TaskStatus::Done,
            Err(payload) => TaskStatus::Failed(panic_message(&payload)),
        };

        // Cooperative cancellation is sticky: once the host cancelled the
        // task, keep reporting `Cancelled` even if the thread runs to
        // completion instead of aborting early.
        {
            let mut guard = status.lock().unwrap_or_else(recover);
            if !matches!(*guard, TaskStatus::Cancelled) {
                *guard = final_status;
            }
        }

        // Signal completion *after* the final status is visible, so a waiter
        // that wakes on `done` always observes the final status.
        let (lock, cvar) = &**done;
        let mut finished = lock.lock().unwrap_or_else(recover);
        *finished = true;
        cvar.notify_all();
    }
}

/// Extracts the message of a panic payload.
///
/// `panic!("...")` produces a `&str` payload and `panic!(String)` a `String`
/// payload; both are surfaced verbatim. Any other payload type (numbers,
/// structs, ...) cannot be turned into text portably (there is no
/// `type_name` on `dyn Any + Send`), so it is reported with a generic
/// message instead.
fn panic_message(payload: &Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "task panicked (payload is not a string)".to_string()
    }
}

/// Removes every finished entry from `tasks` and returns how many were
/// removed.
///
/// "Finished" means the thread has terminated — checked via the `done` flag,
/// because a [`TaskStatus::Cancelled`] status alone does *not* mean the
/// thread stopped (cancellation is cooperative).
fn prune_finished_entries(tasks: &mut HashMap<TaskId, TaskEntry>) -> usize {
    let before = tasks.len();
    tasks.retain(|_, entry| !entry.is_finished());
    before - tasks.len()
}

impl Service for TaskServiceImpl {
    const NAME: &'static str = "task_service";
}

impl TaskService for TaskServiceImpl {
    fn spawn(
        &self,
        name: &str,
        task: Box<dyn FnOnce() + Send + 'static>,
    ) -> ServiceResult<TaskId> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let status = Arc::new(Mutex::new(TaskStatus::Pending));
        let done = Arc::new((Mutex::new(false), Condvar::new()));

        // Register the entry *before* starting the thread, so a task that
        // finishes immediately can never be observed as "not found". This
        // also bounds the map: once it is large enough, finished entries are
        // swept before the new one is inserted.
        {
            let mut tasks = self.tasks.write().unwrap_or_else(recover);
            if tasks.len() >= AUTO_PRUNE_THRESHOLD {
                prune_finished_entries(&mut tasks);
            }
            tasks.insert(
                id,
                TaskEntry {
                    name: name.to_string(),
                    status: Arc::clone(&status),
                    done: Arc::clone(&done),
                    handle: None,
                },
            );
        }

        // `Builder::spawn` returns a `Result` (unlike `thread::spawn`, which
        // panics on failure), so a failed spawn can be reported as an error
        // instead of unwinding through the caller.
        let thread_status = Arc::clone(&status);
        let thread_done = Arc::clone(&done);
        let spawn = std::thread::Builder::new()
            .name(name.to_string())
            .spawn(move || Self::run_task(&thread_status, &thread_done, task));

        match spawn {
            Ok(handle) => {
                let mut tasks = self.tasks.write().unwrap_or_else(recover);
                if let Some(entry) = tasks.get_mut(&id) {
                    entry.handle = Some(handle);
                }
                // If the entry is already gone, the thread had finished and
                // a concurrent spawn swept it; dropping the handle detaches
                // the finished thread, which is harmless.
                Ok(id)
            }
            Err(error) => {
                // The thread never started; roll the entry back so no dead
                // task is left behind.
                let mut tasks = self.tasks.write().unwrap_or_else(recover);
                tasks.remove(&id);
                Err(ServiceError::Other(format!(
                    "failed to spawn task thread: {error}"
                )))
            }
        }
    }

    fn report_progress(&self, id: TaskId, progress: f32) -> ServiceResult<()> {
        let tasks = self.tasks.read().unwrap_or_else(recover);
        let entry = tasks
            .get(&id)
            .ok_or(ServiceError::TaskNotFound(id))?;
        let mut status = entry.status.lock().unwrap_or_else(recover);
        if let TaskStatus::Running(_) = *status {
            // Clamp into [0.0, 1.0]. `f32::clamp` would pass NaN straight
            // through, so NaN is pinned to 0.0; ±infinity clamp to the
            // nearest bound (1.0 / 0.0).
            let progress = if progress.is_nan() {
                0.0
            } else {
                progress.clamp(0.0, 1.0)
            };
            *status = TaskStatus::Running(progress);
        }
        Ok(())
    }

    fn status(&self, id: TaskId) -> Option<TaskStatus> {
        let tasks = self.tasks.read().unwrap_or_else(recover);
        let entry = tasks.get(&id)?;
        Some(entry.status.lock().unwrap_or_else(recover).clone())
    }

    fn cancel(&self, id: TaskId) -> ServiceResult<()> {
        let tasks = self.tasks.read().unwrap_or_else(recover);
        let entry = tasks
            .get(&id)
            .ok_or(ServiceError::TaskNotFound(id))?;
        let mut status = entry.status.lock().unwrap_or_else(recover);
        // Cooperative cancellation: only a live task can be cancelled.
        // Finished tasks are left untouched, so cancelling after completion
        // is a no-op.
        if matches!(
            *status,
            TaskStatus::Pending | TaskStatus::Running(_)
        ) {
            *status = TaskStatus::Cancelled;
        }
        Ok(())
    }

    fn join(&self, id: TaskId) -> ServiceResult<TaskStatus> {
        // Wait for the thread's completion signal *without* holding the map
        // lock, so the task body can call back into the service while we
        // block, and so a concurrent spawn/prune is never stalled by us.
        let done = {
            let tasks = self.tasks.read().unwrap_or_else(recover);
            let entry = tasks
                .get(&id)
                .ok_or(ServiceError::TaskNotFound(id))?;
            Arc::clone(&entry.done)
        };
        {
            let (lock, cvar) = &*done;
            let mut finished = lock.lock().unwrap_or_else(recover);
            while !*finished {
                finished = cvar.wait(finished).unwrap_or_else(recover);
            }
        }

        // The thread has finished; reap its handle if we still own it.
        let handle = {
            let mut tasks = self.tasks.write().unwrap_or_else(recover);
            let entry = tasks
                .get_mut(&id)
                .ok_or(ServiceError::TaskNotFound(id))?;
            entry.handle.take()
        };
        if let Some(handle) = handle {
            // Returns immediately: the thread already exited. The `Err` arm
            // (thread panicked) is expected — the status carries the
            // `Failed` detail.
            let _ = handle.join();
        }
        self.status(id).ok_or(ServiceError::TaskNotFound(id))
    }

    fn join_timeout(
        &self,
        id: TaskId,
        timeout: std::time::Duration,
    ) -> ServiceResult<Option<TaskStatus>> {
        let done = {
            let tasks = self.tasks.read().unwrap_or_else(recover);
            let entry = tasks
                .get(&id)
                .ok_or(ServiceError::TaskNotFound(id))?;
            Arc::clone(&entry.done)
        };

        let deadline = std::time::Instant::now() + timeout;
        let (lock, cvar) = &*done;
        let mut finished = lock.lock().unwrap_or_else(recover);
        while !*finished {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            let (guard, waited) =
                cvar.wait_timeout(finished, remaining).unwrap_or_else(recover);
            finished = guard;
            // If the flag flips in the same instant as the timeout, report
            // the completion rather than a spurious "still running".
            if waited.timed_out() && !*finished {
                return Ok(None);
            }
        }
        drop(finished);

        // Finished: reap the handle if present, then report the final status.
        let handle = {
            let mut tasks = self.tasks.write().unwrap_or_else(recover);
            let entry = tasks
                .get_mut(&id)
                .ok_or(ServiceError::TaskNotFound(id))?;
            entry.handle.take()
        };
        if let Some(handle) = handle {
            let _ = handle.join();
        }
        self.status(id)
            .map(Some)
            .ok_or(ServiceError::TaskNotFound(id))
    }

    fn tasks(&self) -> Vec<(TaskId, String, TaskStatus)> {
        let tasks = self.tasks.read().unwrap_or_else(recover);
        tasks
            .iter()
            .map(|(id, entry)| {
                let status = entry.status.lock().unwrap_or_else(recover).clone();
                (*id, entry.name.clone(), status)
            })
            .collect()
    }

    fn prune_finished(&self) -> ServiceResult<usize> {
        let mut tasks = self.tasks.write().unwrap_or_else(recover);
        Ok(prune_finished_entries(&mut tasks))
    }
}

/// Installs the `task_service` Cordis service.
pub fn plugin() -> PluginHandle {
    service_sync::<TaskServiceImpl, (), _>(
        "task_service",
        Inject::none(),
        |ctx, _config| Ok(TaskServiceImpl::new(ctx)),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    fn service() -> TaskServiceImpl {
        TaskServiceImpl::new(Context::new())
    }

    /// Spawns a task that signals `tx` once its body is running, then sleeps
    /// for `sleep`. Returns `(id, rx)`.
    fn spawn_slow(svc: &TaskServiceImpl, name: &str, sleep: Duration) -> (TaskId, mpsc::Receiver<()>) {
        let (tx, rx) = mpsc::channel();
        let id = svc
            .spawn(
                name,
                Box::new(move || {
                    let _ = tx.send(());
                    std::thread::sleep(sleep);
                }),
            )
            .unwrap();
        rx.recv_timeout(Duration::from_secs(2))
            .expect("task should start");
        (id, rx)
    }

    #[test]
    fn spawn_empty_task_joins_done() {
        let svc = service();
        let id = svc.spawn("empty", Box::new(|| {})).unwrap();
        assert_eq!(svc.join(id).unwrap(), TaskStatus::Done);
        // A second join reads the cached status.
        assert_eq!(svc.join(id).unwrap(), TaskStatus::Done);
    }

    #[test]
    fn spawn_panicking_task_fails() {
        let svc = service();
        let id = svc
            .spawn("boom", Box::new(|| panic!("kaboom")))
            .unwrap();
        match svc.join(id).unwrap() {
            TaskStatus::Failed(msg) => assert!(msg.contains("kaboom"), "unexpected msg: {msg}"),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn panic_str_and_string_payloads_are_extracted_verbatim() {
        let svc = service();
        // `panic!("...")` produces a `&str` payload.
        let id = svc.spawn("str", Box::new(|| panic!("str boom"))).unwrap();
        match svc.join(id).unwrap() {
            TaskStatus::Failed(msg) => assert_eq!(msg, "str boom", "&str payload"),
            other => panic!("expected Failed, got {other:?}"),
        }
        // `panic!(String)` produces a `String` payload.
        let id = svc
            .spawn(
                "string",
                Box::new(|| std::panic::panic_any(String::from("string boom"))),
            )
            .unwrap();
        match svc.join(id).unwrap() {
            TaskStatus::Failed(msg) => assert_eq!(msg, "string boom", "String payload"),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn panic_with_non_string_payload_is_still_failed() {
        let svc = service();
        let id = svc
            .spawn("non-str", Box::new(|| std::panic::panic_any(42u64)))
            .unwrap();
        match svc.join(id).unwrap() {
            TaskStatus::Failed(msg) => {
                assert!(
                    msg.contains("payload"),
                    "generic message expected, got: {msg}"
                )
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn report_progress_updates_running_status() {
        let svc = service();
        let (id, _rx) = spawn_slow(&svc, "slow", Duration::from_millis(80));
        svc.report_progress(id, 1.5).unwrap();
        assert_eq!(svc.status(id), Some(TaskStatus::Running(1.0)));
        svc.report_progress(id, -0.5).unwrap();
        assert_eq!(svc.status(id), Some(TaskStatus::Running(0.0)));
        svc.report_progress(id, 0.42).unwrap();
        assert_eq!(svc.status(id), Some(TaskStatus::Running(0.42)));

        assert_eq!(svc.join(id).unwrap(), TaskStatus::Done);
    }

    #[test]
    fn report_progress_handles_non_finite_values() {
        let svc = service();
        let (id, _rx) = spawn_slow(&svc, "nan", Duration::from_millis(80));
        svc.report_progress(id, f32::NAN).unwrap();
        assert_eq!(svc.status(id), Some(TaskStatus::Running(0.0)));
        svc.report_progress(id, f32::INFINITY).unwrap();
        assert_eq!(svc.status(id), Some(TaskStatus::Running(1.0)));
        svc.report_progress(id, f32::NEG_INFINITY).unwrap();
        assert_eq!(svc.status(id), Some(TaskStatus::Running(0.0)));
        assert_eq!(svc.join(id).unwrap(), TaskStatus::Done);
    }

    #[test]
    fn cancel_marks_cancelled_and_join_stays_cancelled() {
        let svc = service();
        let (id, _rx) = spawn_slow(&svc, "long", Duration::from_millis(200));

        svc.cancel(id).unwrap();
        assert_eq!(svc.status(id), Some(TaskStatus::Cancelled));
        // Join blocks until the cooperative task finishes; the final status
        // is still Cancelled (the thread body is not force-stopped).
        assert_eq!(svc.join(id).unwrap(), TaskStatus::Cancelled);
    }

    #[test]
    fn cancel_after_completion_is_a_noop() {
        let svc = service();
        let id = svc.spawn("quick", Box::new(|| {})).unwrap();
        assert_eq!(svc.join(id).unwrap(), TaskStatus::Done);
        svc.cancel(id).unwrap();
        assert_eq!(svc.status(id), Some(TaskStatus::Done));
    }

    #[test]
    fn join_timeout_waits_with_timeout() {
        let svc = service();
        let (id, _rx) = spawn_slow(&svc, "slow", Duration::from_millis(200));

        // Still running: a short timeout yields None, and the task keeps
        // running (a later join still works).
        assert_eq!(
            svc.join_timeout(id, Duration::from_millis(30)).unwrap(),
            None
        );
        // Once finished, the final status comes back.
        assert_eq!(
            svc.join_timeout(id, Duration::from_secs(5)).unwrap(),
            Some(TaskStatus::Done)
        );
        // The result is cached: a second call returns immediately.
        assert_eq!(
            svc.join_timeout(id, Duration::from_millis(1)).unwrap(),
            Some(TaskStatus::Done)
        );
    }

    #[test]
    fn prune_finished_removes_only_finished_tasks() {
        let svc = service();
        let (running_id, _rx) = spawn_slow(&svc, "running", Duration::from_millis(200));
        let done_id = svc.spawn("done", Box::new(|| {})).unwrap();
        assert_eq!(svc.join(done_id).unwrap(), TaskStatus::Done);

        assert_eq!(svc.prune_finished().unwrap(), 1, "only the done task");
        assert!(svc.status(done_id).is_none(), "pruned task is gone");
        assert!(
            matches!(svc.status(running_id), Some(TaskStatus::Running(_))),
            "running task is kept"
        );
        // A pruned id can no longer be joined.
        assert!(matches!(
            svc.join(done_id),
            Err(ServiceError::TaskNotFound(_))
        ));

        // The running task is still fully operational.
        svc.cancel(running_id).unwrap();
        assert_eq!(svc.join(running_id).unwrap(), TaskStatus::Cancelled);
        // Finishing it makes it prunable.
        assert_eq!(svc.prune_finished().unwrap(), 1);
        assert!(svc.status(running_id).is_none());
    }

    #[test]
    fn spawn_auto_prunes_finished_tasks_over_threshold() {
        let svc = service();
        // Fill the map up to the threshold with tasks that finish quickly.
        // While the count is below the threshold no sweeping happens, so
        // every entry stays joinable.
        let mut ids = Vec::new();
        for i in 0..AUTO_PRUNE_THRESHOLD {
            ids.push(svc.spawn(&format!("t{i}"), Box::new(|| {})).unwrap());
        }
        for id in &ids {
            assert_eq!(svc.join(*id).unwrap(), TaskStatus::Done);
        }
        assert_eq!(svc.tasks().len(), AUTO_PRUNE_THRESHOLD);

        // The next spawn crosses the threshold and sweeps the finished
        // entries before inserting the new one.
        let fresh = svc.spawn("fresh", Box::new(|| {})).unwrap();
        assert_eq!(svc.join(fresh).unwrap(), TaskStatus::Done);
        assert_eq!(
            svc.tasks().len(),
            1,
            "auto-prune should drop the finished tasks"
        );
    }

    #[test]
    fn missing_task_errors() {
        let svc = service();
        assert!(matches!(
            svc.cancel(999),
            Err(ServiceError::TaskNotFound(999))
        ));
        assert!(matches!(
            svc.join(999),
            Err(ServiceError::TaskNotFound(999))
        ));
        assert!(matches!(
            svc.join_timeout(999, Duration::from_millis(1)),
            Err(ServiceError::TaskNotFound(999))
        ));
        assert!(matches!(
            svc.report_progress(999, 0.5),
            Err(ServiceError::TaskNotFound(999))
        ));
        assert!(svc.status(999).is_none());
        // Pruning an empty map is a no-op.
        assert_eq!(svc.prune_finished().unwrap(), 0);
    }

    #[test]
    fn tasks_snapshot_lists_entries() {
        let svc = service();
        let id = svc.spawn("snap", Box::new(|| {})).unwrap();
        let _ = svc.join(id).unwrap();
        let snapshot = svc.tasks();
        assert!(
            snapshot
                .iter()
                .any(|(tid, name, _)| *tid == id && name == "snap"),
            "snapshot should contain the task: {snapshot:?}"
        );
    }

    #[test]
    fn join_does_not_block_other_service_calls() {
        let svc = Arc::new(service());
        let (id, _rx) = spawn_slow(&svc, "slow", Duration::from_millis(400));

        // A second thread blocks inside `join` until the slow task ends.
        let joiner_svc = Arc::clone(&svc);
        let joiner = std::thread::spawn(move || joiner_svc.join(id).unwrap());

        // While the joiner is blocked, other calls (report_progress, spawn,
        // join of a different task) must complete without waiting for the
        // slow task — `join` must not hold the map lock while blocking.
        let worker_svc = Arc::clone(&svc);
        let worker = std::thread::spawn(move || {
            worker_svc.report_progress(id, 0.5).unwrap();
            let other = worker_svc.spawn("other", Box::new(|| {})).unwrap();
            worker_svc.join(other).unwrap()
        });
        worker.join().unwrap();

        // The slow task is still running at this point (its 400 ms sleep has
        // not elapsed), so its progress update was applied, not shadowed.
        assert_eq!(svc.status(id), Some(TaskStatus::Running(0.5)));
        assert_eq!(joiner.join().unwrap(), TaskStatus::Done);
    }

    #[test]
    fn ids_are_unique_and_start_at_one() {
        let svc = service();
        let a = svc.spawn("a", Box::new(|| {})).unwrap();
        let b = svc.spawn("b", Box::new(|| {})).unwrap();
        assert_eq!(a, 1);
        assert_eq!(b, 2);
        assert_ne!(a, b);
        let _ = svc.join(a).unwrap();
        let _ = svc.join(b).unwrap();
    }
}
