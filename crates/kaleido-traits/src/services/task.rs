//! The **task** manager — background task tracking.

use super::ServiceResult;

/// Opaque handle for a spawned background task.
///
/// Allocated monotonically from 1 by [`TaskService::spawn`] and stays
/// valid until the task is pruned.
pub type TaskId = u64;

/// The lifecycle state of a background task.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    /// Queued but has not started running yet.
    Pending,
    /// Currently executing; carries progress in `[0.0, 1.0]`.
    Running(f32),
    /// Completed successfully.
    Done,
    /// Cancelled by the host (cooperative — the thread may still be running).
    Cancelled,
    /// The task body panicked; carries the panic message when available.
    Failed(String),
}

/// The background-task management service.
///
/// Long-running operations (export, AI jobs, plugin installs) are spawned
/// through this service so the host can show progress and cancel them.
///
/// # Cancellation model
///
/// Cancellation is **cooperative** and **sticky**: [`Self::cancel`] only
/// records the request; it never interrupts the running thread. Once
/// cancelled, a task's final status is [`TaskStatus::Cancelled`] even if
/// the body runs to completion.
pub trait TaskService: Send + Sync + 'static {
    // ── Lifecycle ────────────────────────────────────────────────────────

    /// Spawns a background task.
    ///
    /// The task body runs on its own thread. A unique [`TaskId`] is returned
    /// for tracking, progress reporting, cancellation, and joining.
    fn spawn(
        &self,
        name: &str,
        task: Box<dyn FnOnce() + Send + 'static>,
    ) -> ServiceResult<TaskId>;

    /// Reports progress for a running task, clamped to `[0.0, 1.0]`.
    ///
    /// Non-finite values (`NaN`, ±infinity) are treated as `0.0` / clamped
    /// to the nearest bound. Only tasks in [`TaskStatus::Running`] accept
    /// updates; calls on tasks in other states are no-ops.
    fn report_progress(&self, id: TaskId, progress: f32) -> ServiceResult<()>;

    // ── Query ────────────────────────────────────────────────────────────

    /// Returns the current status of a task, or `None` when the id is
    /// unknown (task never existed or has been pruned).
    fn status(&self, id: TaskId) -> Option<TaskStatus>;

    /// Returns a snapshot of all tracked tasks as `(id, name, status)` tuples.
    fn tasks(&self) -> Vec<(TaskId, String, TaskStatus)>;

    // ── Control ──────────────────────────────────────────────────────────

    /// Requests cooperative cancellation of a task.
    ///
    /// Only live tasks ([`TaskStatus::Pending`] or [`TaskStatus::Running`])
    /// are affected; cancelling a finished task is a no-op.
    fn cancel(&self, id: TaskId) -> ServiceResult<()>;

    /// Blocks until the task finishes, then returns its final status.
    fn join(&self, id: TaskId) -> ServiceResult<TaskStatus>;

    /// Waits up to `timeout` for the task to finish.
    ///
    /// Returns `Ok(None)` on timeout, `Ok(Some(status))` when the task
    /// finishes within the deadline, or `Err` when the id is unknown.
    fn join_timeout(&self, id: TaskId, timeout: std::time::Duration) -> ServiceResult<Option<TaskStatus>>;

    // ── Cleanup ──────────────────────────────────────────────────────────

    /// Removes every finished task and returns how many were pruned.
    fn prune_finished(&self) -> ServiceResult<usize>;
}
