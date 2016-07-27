use std::fmt::Write;

use serde_json::Value;

use error::Result;

// marker trait for messages that client can initiate connection with
pub trait Command {}

#[derive(Debug)]
pub struct Query {
    id: Option<String>,
    query: String,
    trace_id: Option<String>,
    auth: Option<String>,
    config: Value,
}

#[derive(Debug)]
pub struct Authenticate {
    user: String,
    key: String,
    trace_id: Option<String>,
}

impl Query {
    pub fn get_config<'a>(&'a self, key: &str) -> Result<&'a Value> {
        let v = try!(self.config.search(key).ok_or(format!("missing key {} in query config", key)));
        Ok(v)
    }

    pub fn get_opt<'a>(&'a self, key: &str) -> Option<&'a Value> {
        self.config.search(key)
    }
}

impl Command for Authenticate {}
impl Command for Query {}

#[derive(Debug)]
pub enum QueryStatus {
    Queued,
    Started,
    Running,
    Waiting,
    Finished,
}


#[derive(Debug)]
pub enum SonicMessage {
    // client ~> server
    Acknowledge,

    QueryMsg(Query),

    AuthenticateMsg(Authenticate),

    // client <~ server
    TypeMetadata(Vec<(String, Value)>),

    QueryProgress {
        status: QueryStatus,
        progress: f64,
        total: Option<f64>,
        units: Option<String>,
    },

    OutputChunk(Vec<Value>),

    Done(Option<String>),
}

impl SonicMessage {
    // DoneWithQueryExecution error
    pub fn done<T>(e: Result<T>) -> SonicMessage {
        let variation = match e {
            Ok(_) => None,
            Err(e) => Some(format!("{}", e).to_owned()),
        };
        SonicMessage::Done(variation).into()
    }

    pub fn kind(&self) -> protocol::MessageKind {
        match *self {
            Acknowledge => protocol::MessageKind::AcknowledgeKind,

            QueryMsg(_) => protocol::MessageKind::QueryKind,

            AuthenticateMsg(_) => protocol::MessageKind::AuthKind,

            // client <~ server
            &TypeMetadata(_) => MessageKind::TypeMetadataKind,

            &QueryProgress{ .. } => MessageKind::ProgressKind,

            &OutputChunk(_) => MessageKind::OutputKind,

            &Done(_) => MessageKind::DoneKind,

        }
    }

    pub fn into_json(self) -> Value {
        let msg: protocol::ProtoSonicMessage = From::from(self);

        ::serde_json::to_value(&msg)
    }

    pub fn from_slice(slice: &[u8]) -> Result<SonicMessage> {
        let msg = try!(::serde_json::from_slice::<protocol::ProtoSonicMessage>(slice));
        msg.into_msg()
    }

    pub fn from_bytes(buf: Vec<u8>) -> Result<SonicMessage> {
        Self::from_slice(buf.as_slice())
    }

    pub fn into_bytes(self) -> Result<Vec<u8>> {
        let s = try!(::serde_json::to_string(&self.into_json()));
        Ok(s.into_bytes())
    }
}

pub mod protocol {
    #[cfg(feature = "serde_macros")]
    include!("protocol.rs.in");

    #[cfg(not(feature = "serde_macros"))]
    include!(concat!(env!("OUT_DIR"), "/protocol.rs"));
}
