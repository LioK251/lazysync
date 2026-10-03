pub mod credentials;
#[cfg(feature = "desktop")]
pub mod desktop;
#[cfg(feature = "desktop")]
mod desktop_window;
pub mod git;
pub mod github;
pub mod ignore;
pub mod models;
pub mod paths;
pub mod runtime;
pub mod storage;
pub mod sync;
#[cfg(test)]
mod tests;
pub mod watcher;
pub mod window_state;
