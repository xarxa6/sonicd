use std::collections::BTreeMap;
use serde_json::Value;
use error::{Result, ErrorKind};
use super::*;

#[derive(Serialize, Deserialize, Debug)]
pub enum MessageKind {
    // client ~> server
    #[serde(rename="A")]
    AcknowledgeKind,
    #[serde(rename="Q")]
    QueryKind,
    #[serde(rename="H")]
    AuthKind,
    // client <~ server
    #[serde(rename="T")]
    TypeMetadataKind,
    #[serde(rename="P")]
    ProgressKind,
    #[serde(rename="O")]
    OutputKind,
    #[serde(rename="D")]
    DoneKind,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SonicMessage {
    #[serde(rename="e")]
    pub event_type: MessageKind,
    #[serde(rename="v")]
    pub variation: Option<String>,
    #[serde(rename="p")]
    pub payload: Option<Value>,
}

impl SonicMessage {
    // DoneWithQueryExecution error
    pub fn done<T>(e: Result<T>) -> SonicMessage {
        let variation = match e {
            Ok(_) => None,
            Err(e) => Some(format!("{}", e).to_owned()),
        };
        Done(variation).into()
    }

    pub fn into<T: SonicMessageLike<T>>(self) -> Result<T> {
        T::from(self)
    }

    pub fn from_slice(slice: &[u8]) -> Result<SonicMessage> {
        let msg = try!(::serde_json::from_slice::<SonicMessage>(slice));
        Ok(msg)
    }

    pub fn from_bytes(buf: Vec<u8>) -> Result<SonicMessage> {
        Self::from_slice(buf.as_slice())
    }

    pub fn into_json(self) -> Value {
        ::serde_json::to_value::<SonicMessage>(&self)
    }

    pub fn into_bytes(self) -> Result<Vec<u8>> {
        let s = try!(::serde_json::to_string(&self));
        Ok(s.into_bytes())
    }
}

impl From<Query> for SonicMessage {
    fn from(msg: Query) -> Self {
        let Query { config, query, auth, trace_id, .. } = msg;
        let mut payload = BTreeMap::new();

        payload.insert("config".to_owned(), config);
        payload.insert("auth".to_owned(),
                       auth.map(|s| Value::String(s))
                           .unwrap_or_else(|| Value::Null));
        payload.insert("trace_id".to_owned(),
                       trace_id.map(|s| Value::String(s))
                           .unwrap_or_else(|| Value::Null));

        SonicMessage {
            event_type: MessageKind::QueryKind,
            variation: Some(query),
            payload: Some(Value::Object(payload)),
        }
    }
}

impl From<Acknowledge> for SonicMessage {
    fn from(_: Acknowledge) -> Self {
        SonicMessage {
            event_type: MessageKind::AcknowledgeKind,
            variation: None,
            payload: None,
        }
    }
}

impl From<Authenticate> for SonicMessage {
    fn from(msg: Authenticate) -> Self {
        let mut payload = BTreeMap::new();

        payload.insert("user".to_owned(), Value::String(msg.user));

        SonicMessage {
            event_type: MessageKind::AuthKind,
            variation: Some(msg.key),
            payload: Some(Value::Object(payload)),
        }
    }
}

impl From<TypeMetadata> for SonicMessage {
    fn from(msg: TypeMetadata) -> Self {
        SonicMessage {
            event_type: MessageKind::TypeMetadataKind,
            variation: None,
            payload: Some(::serde_json::to_value(&msg.0)),
        }
    }
}

impl From<QueryProgress> for SonicMessage {
    fn from(msg: QueryProgress) -> Self {
        let mut payload = BTreeMap::new();

        payload.insert("p".to_owned(), Value::F64(msg.progress));

        msg.units.map(|u| payload.insert("u".to_owned(), Value::String(u)));
        msg.total.map(|t| payload.insert("t".to_owned(), Value::F64(t)));

        SonicMessage {
            event_type: MessageKind::ProgressKind,
            variation: None,
            payload: Some(Value::Object(payload)),
        }
    }
}

impl From<OutputChunk> for SonicMessage {
    fn from(msg: OutputChunk) -> Self {
        SonicMessage {
            event_type: MessageKind::OutputKind,
            variation: None,
            payload: Some(Value::Array(msg.0)),
        }
    }
}

impl From<Done> for SonicMessage {
    fn from(msg: Done) -> Self {
        SonicMessage {
            event_type: MessageKind::DoneKind,
            variation: msg.0,
            payload: None,
        }
    }
}

fn get_payload(payload: Option<Value>) -> Result<BTreeMap<String, Value>> {
    match payload {
        Some(Value::Object(p)) => Ok(p),
        _ => try!(Err(ErrorKind::Proto("msg payload is empty".to_owned()))),
    }
}

pub trait SonicMessageLike<T: From<T>> {
    fn from(SonicMessage) -> Result<T>;
}

impl SonicMessageLike<Acknowledge> for Acknowledge {
    fn from(_: SonicMessage) -> Result<Acknowledge> {
        Ok(Acknowledge)
    }
}

impl SonicMessageLike<Authenticate> for Authenticate {
    fn from(msg: SonicMessage) -> Result<Authenticate> {
        let payload = try!(get_payload(msg.payload));
        let user = try!(payload.get("user")
            .and_then(|s| s.as_string().map(|s| s.to_owned()))
            .ok_or_else(|| ErrorKind::Proto("missing user field in payload".to_owned())));
        let key = try!(payload.get("key")
            .and_then(|s| s.as_string().map(|s| s.to_owned()))
            .ok_or_else(|| ErrorKind::Proto("missing key field in payload".to_owned())));
        let trace_id = payload.get("trace_id").and_then(|s| s.as_string().map(|s| s.to_owned()));

        Ok(Authenticate {
            user: user,
            key: key,
            trace_id: trace_id,
        })
    }
}

impl SonicMessageLike<TypeMetadata> for TypeMetadata {
    fn from(msg: SonicMessage) -> Result<TypeMetadata> {
        let payload = try!(msg.payload
            .ok_or_else(|| ErrorKind::Proto("msg payload is empty".to_owned())));

        let data = try!(::serde_json::from_value(payload));
        Ok(TypeMetadata(data))
    }
}

impl SonicMessageLike<QueryProgress> for QueryProgress {
    fn from(msg: SonicMessage) -> Result<QueryProgress> {
        let payload = try!(get_payload(msg.payload));

        let total = payload.get("t").and_then(|s| s.as_f64());

        let js = try!(payload.get("s")
            .ok_or_else(|| ErrorKind::Proto("missing query status in payload".to_owned())));

        let status = match js {
            &Value::U64(0) => QueryStatus::Queued,
            &Value::U64(1) => QueryStatus::Started,
            &Value::U64(2) => QueryStatus::Running,
            &Value::U64(3) => QueryStatus::Waiting,
            s => {
                return Err(ErrorKind::Proto(format!("unexpected query status {:?}", s)).into());
            }
        };

        let progress = try!(payload.get("p")
            .and_then(|s| s.as_f64())
            .ok_or_else(|| ErrorKind::Proto("progress not found in payload".to_owned())));

        let units = payload.get("u").and_then(|s| s.as_string().map(|s| s.to_owned()));

        Ok(QueryProgress {
            progress: progress,
            status: status,
            total: total,
            units: units,
        })
    }
}

impl SonicMessageLike<OutputChunk> for OutputChunk {
    fn from(msg: SonicMessage) -> Result<OutputChunk> {
        match msg.payload {
            Some(Value::Array(data)) => Ok(OutputChunk(data)),
            s => try!(Err(ErrorKind::Proto(format!("payload is not an array: {:?}", s)))),
        }
    }
}

impl SonicMessageLike<Done> for Done {
    fn from(msg: SonicMessage) -> Result<Done> {
        Ok(Done(msg.variation))
    }
}

impl SonicMessageLike<Query> for Query {
    fn from(msg: SonicMessage) -> Result<Query> {

        let payload = try!(get_payload(msg.payload));

        let trace_id = payload.get("trace_id")
            .and_then(|t| t.as_string().map(|t| t.to_owned()));

        let auth_token = payload.get("auth")
            .and_then(|a| a.as_string().map(|a| a.to_owned()));

        let query = try!(msg.variation
            .ok_or_else(|| ErrorKind::Proto("msg variation is empty".to_owned())));

        let config = try!(payload.get("config")
            .map(|c| c.to_owned())
            .ok_or_else(|| {
                ErrorKind::Proto("missing 'config' in query message payload".to_owned())
            }));

        Ok(Query {
            id: None,
            trace_id: trace_id,
            query: query,
            auth: auth_token,
            config: config,
        })
    }
}
