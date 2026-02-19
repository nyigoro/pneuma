use anyhow::Result;
use serde_json::Value;

pub fn invoke_host_function(name: &str, args: Vec<Value>) -> Result<Value> {
    Ok(Value::String(format!("stub:{name}({} args)", args.len())))
}
