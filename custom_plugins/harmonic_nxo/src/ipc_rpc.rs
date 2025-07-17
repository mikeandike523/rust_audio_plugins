use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

pub type RpcFn = dyn Fn(Vec<Value>) -> Value + Send + Sync;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RpcAction {
    SetRequestedFunction { id: u32, name: String, args: Vec<Value> },
    SetFunctionResult { id: u32, result: Value },
    SendFunctionResult { id: u32 },
}

pub struct IpcRpc {
    funcs: HashMap<String, Box<RpcFn>>,
    pending_send: HashMap<u32, Value>,
    incoming: HashMap<u32, Value>,
    next_id: u32,
}

impl IpcRpc {
    pub fn new() -> Self {
        Self {
            funcs: HashMap::new(),
            pending_send: HashMap::new(),
            incoming: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn register_function<F>(&mut self, name: impl Into<String>, func: F)
    where
        F: Fn(Vec<Value>) -> Value + Send + Sync + 'static,
    {
        self.funcs.insert(name.into(), Box::new(func));
    }

    pub fn handle_action<F>(&mut self, action: RpcAction, mut send: F)
    where
        F: FnMut(RpcAction),
    {
        match action {
            RpcAction::SetRequestedFunction { id, name, args } => {
                if let Some(func) = self.funcs.get(&name) {
                    let result = func(args);
                    self.pending_send.insert(id, result);
                } else {
                    self.pending_send.insert(id, Value::Null);
                }
            }
            RpcAction::SendFunctionResult { id } => {
                if let Some(result) = self.pending_send.remove(&id) {
                    send(RpcAction::SetFunctionResult { id, result });
                }
            }
            RpcAction::SetFunctionResult { id, result } => {
                self.incoming.insert(id, result);
            }
        }
    }

    pub fn call_remote_function<F>(&mut self, mut send: F, name: impl Into<String>, args: Vec<Value>) -> u32
    where
        F: FnMut(RpcAction),
    {
        let id = self.next_id;
        self.next_id += 1;
        send(RpcAction::SetRequestedFunction {
            id,
            name: name.into(),
            args,
        });
        send(RpcAction::SendFunctionResult { id });
        id
    }

    pub fn take_result(&mut self, id: u32) -> Option<Value> {
        self.incoming.remove(&id)
    }
}
