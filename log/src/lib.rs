// Copyright (c) 2022, 37 Miners, LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Logging crate used with nioruntime. The crate has an extensive macro
//! library that allows for logging at the standard 5 levels and also
//! allows for specifying a log file and various options. All options can
//! be seen in the [`crate::LogConfig`] struct. This crate is largely compatible
//! with the [log](https://docs.rs/log/latest/log/) crate. So any code
//! that was written to work with that crate will work with this crate.
//! In addition to the [`trace`], [`debug`], [`info`], [`warn`], [`error`]
//! and [`fatal`] log levels, this crate provides an 'all' version and 'no_ts'
//! version of each macro. For example: [`info_all`] and [`info_no_ts`].
//! These macros allow for logging to standard out, no matter how the log is
//! configured and log without the timestamp respectively. The main difference
//! is that this crate returns errors so you will have to add the error handling
//! which can be as simple as using the question mark operator.
//!
//! The output will look something like this:
//!
//! ```text
//! [2022-02-24 13:52:24]: (FATAL) [..ioruntime/src/main.rs:116]: fatal
//! [2022-02-24 13:52:24]: (ERROR) [..ioruntime/src/main.rs:120]: error
//! [2022-02-24 13:52:24]: (WARN) [..ioruntime/src/main.rs:124]: warn
//! [2022-02-24 13:52:24]: (INFO) [..ioruntime/src/main.rs:128]: info
//! [2022-02-24 13:52:24]: (DEBUG) [..ioruntime/src/main.rs:132]: debug
//! [2022-02-24 13:52:24]: (TRACE) [..ioruntime/src/main.rs:136]: trace
//! ```
//!
//! # Examples
//!
//! ```
//! // This is a basic example showing configuration and a single logged line.
//! use nioruntime_log::*;
//! use nioruntime_err::Error;
//!
//! debug!(); // each file must set the log level before calling the macro.
//!           // this can be done at the top of the file and changed at any
//!           // scope level throughout the file.
//!
//! fn test() -> Result<(), Error> {
//!     // if the log_config! macro is not called, a default logger will be used.
//!     log_config!(LogConfig {
//!         file_path: Some("/path/to/mylog.log".to_string()),
//!         max_age_millis: 300_0000, // set log rotations to every 300 seconds (5 minutes)
//!         max_size: 100_000, // set log rotations to every 100,000 bytes
//!         ..Default::default() // use defaults for the rest of the options
//!     });
//!
//!     let value = 1;
//!     info!("This will be logged. Value: {}", value)?;
//!     Ok(())
//! }
//! ```
//!
//! ```
//! // This example shows a log rotation
//! use nioruntime_log::*;
//! use nioruntime_err::Error;
//! info!();
//!
//! fn test() -> Result<(), Error> {
//!     log_config!(LogConfig {
//!         file_path: Some("/path/to/mylog.log".to_string()),
//!         auto_rotate: false, // set to false to show manual rotation
//!         max_size: 10, // set to a very low number to demonstrate
//!         ..Default::default() // use defaults for the rest of the options
//!     });
//!
//!     info!("0")?;
//!
//!     // log less than the max size
//!     let status = rotation_status!()?;
//!     assert_eq!(status, RotationStatus::NotNeeded); // not needed yet
//!
//!     // log enough to push us over the limit
//!     info!("0123456789")?;
//!     let status = rotation_status!()?;
//!     assert_eq!(status, RotationStatus::Needed); // rotation is needed
//!
//!     rotate!(); // do a manual rotation
//!     let status = rotation_status!()?;
//!     assert_eq!(status, RotationStatus::NotNeeded); // now rotation is not needed
//!     
//!     Ok(())
//! }
//! ```
//!
//! ```
//! // This example shows updating the settings
//! use nioruntime_log::*;
//! use nioruntime_err::Error;
//!
//! info!();
//!
//! fn test() -> Result<(), Error> {
//!     log_config!(LogConfig {
//!         file_path: Some("/path/to/mylog.log".to_string()),
//!         show_stdout: true,
//!         ..Default::default()
//!     });
//!
//!     info!("Log with initial settings")?;
//!     set_config_option!(Settings::Stdout, false)?;
//!
//!     info!("Log with updated settings. This will not go to stdout.")?;
//!     set_config_option!(Settings::Stdout, true)?;
//!
//!     info!("This will also go to stdout")?;
//!
//!     Ok(())
//! }
//!
//! ```
//!
//! ```
//! // This example shows all the named logging functions
//! use nioruntime_log::*;
//! use nioruntime_err::Error;
//!
//! info!();
//!
//! fn test() -> Result<(), Error> {
//!     log_config!(LogConfig {
//!         file_path: Some("/path/to/mylog.log".to_string()),
//!         show_stdout: false,
//!         ..Default::default()
//!     });
//!
//!     fatal!("fatal").expect("failed to log");
//!     fatal_no_ts!("fatal_no_ts").expect("failed to log");
//!     fatal_all!("fatal all").expect("failed to log");
//!
//!     error!("error").expect("failed to log");
//!     error_no_ts!("error_no_ts").expect("failed to log");
//!     error_all!("error all").expect("failed to log");
//!
//!     warn!("warn").expect("failed to log");
//!     warn_no_ts!("warn_no_ts").expect("failed to log");
//!     warn_all!("warn all").expect("failed to log");
//!
//!     info!("info").expect("failed to log");
//!     info_no_ts!("info no ts").expect("failed to log");
//!     info_all!("info all").expect("failed to log");
//!
//!     debug!("debug").expect("failed to log");
//!     debug_no_ts!("debug no ts").expect("failed to log");
//!     debug_all!("debug all").expect("failed to log");
//!
//!     trace!("trace").expect("failed to log");
//!     trace_no_ts!("trace_no_ts").expect("failed to log");
//!     trace_all!("trace all").expect("failed to log");
//!
//!     Ok(())
//! }
//! ```
//! # Using in Cargo.toml
//! To use the crate in a project add the following two line to Cargo.toml:
//! ```toml
//! [dependencies]
//! nioruntime_log = { git = "https://github.com/37miners/nioruntime" }
//! ```
//!
//! Optionally you may want to add the nioruntime_err crate to the project:
//! ```toml
//! [dependencies]
//! nioruntime_err = { git = "https://github.com/37miners/nioruntime" }
//! ```

use nioruntime_deps::backtrace;
use nioruntime_deps::chrono;
use nioruntime_deps::colored;
use nioruntime_deps::lazy_static;
use nioruntime_deps::rand;

mod logger;
mod macros;

pub use crate::logger::{do_log, Log, LogConfig, RotationStatus, Settings};
pub use crate::logger::{DEBUG, ERROR, FATAL, INFO, TRACE, WARN};
pub use crate::macros::{DEFAULT_LOG_NAME, STATIC_LOG};

#[doc(hidden)]
pub use nioruntime_deps;
#[doc(hidden)]
pub use nioruntime_err;
