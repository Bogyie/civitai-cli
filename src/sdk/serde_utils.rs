use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::Value;

pub(crate) fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

pub(crate) fn deserialize_stringish_opt<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Option<Value> = Option::deserialize(deserializer)?;
    Ok(match value {
        Some(Value::String(text)) => normalize_optional_string(Some(text)),
        Some(Value::Number(number)) => Some(number.to_string()),
        Some(Value::Bool(value)) => Some(value.to_string()),
        Some(Value::Null) | None => None,
        Some(other) => Some(other.to_string()),
    })
}

pub(crate) fn deserialize_u64ish<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(deserialize_option_u64ish(deserializer)?.unwrap_or(0))
}

pub(crate) fn deserialize_option_u64ish<'de, D>(
    deserializer: D,
) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Option<Value> = Option::deserialize(deserializer)?;
    Ok(match value {
        Some(Value::Number(number)) => number
            .as_u64()
            .or_else(|| number.as_i64().and_then(|value| u64::try_from(value).ok())),
        Some(Value::String(text)) => text.trim().parse::<f64>().ok().and_then(|value| {
            (value.is_finite() && value >= 0.0).then_some(value.round() as u64)
        }),
        Some(Value::Bool(value)) => Some(if value { 1 } else { 0 }),
        _ => None,
    })
}

pub(crate) fn deserialize_f64ish<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(deserialize_option_f64ish(deserializer)?.unwrap_or(0.0))
}

pub(crate) fn deserialize_option_f64ish<'de, D>(
    deserializer: D,
) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Option<Value> = Option::deserialize(deserializer)?;
    Ok(match value {
        Some(Value::Number(number)) => number.as_f64(),
        Some(Value::String(text)) => text.trim().parse::<f64>().ok(),
        Some(Value::Bool(value)) => Some(if value { 1.0 } else { 0.0 }),
        _ => None,
    })
}

pub(crate) fn deserialize_boolish<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Option<Value> = Option::deserialize(deserializer)?;
    Ok(match value {
        Some(Value::Bool(value)) => value,
        Some(Value::Number(number)) => number.as_u64().unwrap_or(0) != 0,
        Some(Value::String(text)) => matches!(
            text.trim().to_ascii_lowercase().as_str(),
            "true" | "1" | "yes"
        ),
        _ => false,
    })
}

pub(crate) fn deserialize_normalized_value_opt<'de, D>(
    deserializer: D,
) -> Result<Option<Value>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Option<Value> = Option::deserialize(deserializer)?;
    Ok(value.map(normalize_jsonish_value))
}

pub(crate) fn deserialize_normalized_vec<'de, D, T>(
    deserializer: D,
) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let value: Option<Value> = Option::deserialize(deserializer)?;
    let Some(value) = value.map(normalize_jsonish_value) else {
        return Ok(Vec::new());
    };
    serde_json::from_value::<Vec<T>>(value).map_err(serde::de::Error::custom)
}

pub(crate) fn deserialize_normalized_struct_opt<'de, D, T>(
    deserializer: D,
) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let value: Option<Value> = Option::deserialize(deserializer)?;
    let Some(value) = value.map(normalize_jsonish_value) else {
        return Ok(None);
    };
    serde_json::from_value::<T>(value)
        .map(Some)
        .map_err(serde::de::Error::custom)
}

pub(crate) fn normalize_jsonish_value(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| (key, normalize_jsonish_value(value)))
                .collect(),
        ),
        Value::Array(items) => {
            Value::Array(items.into_iter().map(normalize_jsonish_value).collect())
        }
        Value::String(raw) => match serde_json::from_str::<Value>(&raw) {
            Ok(Value::Object(map)) => normalize_jsonish_value(Value::Object(map)),
            Ok(Value::Array(items)) => normalize_jsonish_value(Value::Array(items)),
            _ => Value::String(raw),
        },
        other => other,
    }
}
