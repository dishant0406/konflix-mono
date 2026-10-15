//! The tokio runtime that runs the network work.
//!
//! GPUI has its own executor. reqwest, librqbit and the local server need tokio, so
//! they run here and the UI awaits the result: a `JoinHandle` is an ordinary future and
//! can be awaited from any executor.

use std::future::Future;
use std::sync::OnceLock;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| Runtime::new().expect("tokio runtime starts"))
}

/// Starts `future` on the tokio runtime. Dropping the handle does not stop it; abort it for that.
pub fn spawn<T: Send + 'static>(future: impl Future<Output = T> + Send + 'static) -> JoinHandle<T> {
    runtime().spawn(future)
}

/// Runs `future` on the tokio runtime and returns its output, or `None` if it panicked: a
/// bug while reading one reply from the network must not take the window down with it.
pub async fn run<T: Send + 'static>(future: impl Future<Output = T> + Send + 'static) -> Option<T> {
    match spawn(future).await {
        Ok(output) => Some(output),
        Err(error) => {
            tracing::error!("[runtime] a network task failed: {error}");
            None
        }
    }
}

/// Runs `future` to completion from a synchronous context, for startup and shutdown.
pub fn block_on<T>(future: impl Future<Output = T>) -> T {
    runtime().block_on(future)
}
