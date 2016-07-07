#![cfg_attr(feature = "serde_macros", feature(custom_derive, plugin))]
#![cfg_attr(feature = "serde_macros", plugin(serde_macros))]
extern crate serde;
extern crate serde_json;
extern crate nix;
extern crate byteorder;
#[macro_use] extern crate error_chain;
#[macro_use] extern crate log;

#[cfg(feature="websocket")]
extern crate ws as libws;

mod api;
mod error;
mod model;
pub mod net;
#[macro_use] pub mod io;

//#[cfg(feature="websocket")]
//pub mod ws;

pub use api::{run, stream, authenticate};
pub use model::{Authenticate, Log, Acknowledge, Query, TypeMetadata, Done, OutputChunk, QueryProgress};
pub use model::protocol::SonicMessage;
pub use error::{Result, Error, ErrorKind};

static VERSION: &'static str = env!("CARGO_PKG_VERSION");
