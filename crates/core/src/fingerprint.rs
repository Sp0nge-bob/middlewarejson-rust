use serde_json::Value;

pub fn extract_transport_path(stream: &Value) -> String {
    get_nested(stream, &["wsSettings", "path"])
        .or_else(|| get_nested(stream, &["xhttpSettings", "path"]))
        .or_else(|| get_nested(stream, &["grpcSettings", "serviceName"]))
        .unwrap_or_default()
}

pub fn compute_inbound_fingerprint(outbound: &Value) -> String {
    let protocol = value_as_str(outbound.get("protocol"));
    let settings = outbound
        .get("settings")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let stream = outbound
        .get("streamSettings")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    let _settings_value = Value::Object(settings.clone());
    let stream_value = Value::Object(stream.clone());

    let address = value_as_str(settings.get("address"));
    let port = value_as_str(settings.get("port"));
    let network = value_as_str(stream.get("network"));
    let security = value_as_str(stream.get("security"));
    let path = extract_transport_path(&stream_value);
    let mode = get_nested(&stream_value, &["xhttpSettings", "mode"]).unwrap_or_default();

    format!(
        "{protocol}|{address}|{network}|{path}|{port}|{security}|{mode}"
    )
}

fn get_nested(data: &Value, keys: &[&str]) -> Option<String> {
    let mut current = data;
    for key in keys {
        current = current.get(*key)?;
    }
    if current.is_null() {
        return None;
    }
    Some(value_as_str(Some(current)))
}

fn value_as_str(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
    }
}