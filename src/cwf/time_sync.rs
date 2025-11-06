#![allow(clippy::future_not_send, reason = "single-threaded")]

#[cfg(feature = "wifi")]
pub use crate::time_sync::{TimeSync, TimeSyncEvent, TimeSyncNotifier};

#[cfg(not(feature = "wifi"))]
mod stub {
    use core::future::pending;

    use embassy_executor::Spawner;
    use static_cell::StaticCell;

    use crate::unix_seconds::UnixSeconds;

    /// Events produced by the time synchronisation task.
    #[derive(Clone)]
    pub enum TimeSyncEvent {
        Success { unix_seconds: UnixSeconds },
        Failed(&'static str),
    }

    /// Notifier used to construct a [`TimeSync`] instance.
    pub struct TimeSyncNotifier {
        time_sync_cell: StaticCell<TimeSync>,
    }

    /// Minimal time synchronisation stub that never produces events.
    pub struct TimeSync;

    impl TimeSync {
        /// Create time sync resources.
        #[must_use]
        pub const fn notifier() -> TimeSyncNotifier {
            TimeSyncNotifier {
                time_sync_cell: StaticCell::new(),
            }
        }

        /// Construct the stub device.
        pub fn new(resources: &'static TimeSyncNotifier, _spawner: Spawner) -> &'static Self {
            resources.time_sync_cell.init(Self {})
        }

        /// Wait for the next time sync event. This stub never resolves, effectively disabling sync.
        pub async fn wait(&self) -> TimeSyncEvent {
            pending().await
        }
    }
}

#[cfg(not(feature = "wifi"))]
pub use stub::{TimeSync, TimeSyncEvent, TimeSyncNotifier};
