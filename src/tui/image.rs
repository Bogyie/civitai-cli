use civitai_cli::sdk::SearchImageHit;
use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub struct ParsedImageStats {
    pub reactions: u64,
    pub comments: u64,
    pub collected: u64,
    pub buzz: u64,
    pub likes: u64,
    pub hearts: u64,
}

pub fn image_username(hit: &SearchImageHit) -> Option<String> {
    hit.user
        .as_ref()
        .and_then(|user| user.username.clone())
        .filter(|value| !value.trim().is_empty())
}

pub fn image_media_url(hit: &SearchImageHit) -> Option<String> {
    hit.thumbnail_url
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| hit.original_media_url())
}

pub fn image_prompt(hit: &SearchImageHit) -> Option<String> {
    if hit.hide_meta.unwrap_or(false) {
        return None;
    }

    hit.prompt
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            hit.metadata
                .as_ref()
                .and_then(extract_prompt_from_metadata)
        })
}

pub fn image_stats(hit: &SearchImageHit) -> ParsedImageStats {
    let stats = hit.stats.as_ref();
    ParsedImageStats {
        reactions: value_u64(stats.and_then(|value| value.get("reactionCountAllTime"))),
        comments: value_u64(stats.and_then(|value| value.get("commentCountAllTime"))),
        collected: value_u64(stats.and_then(|value| value.get("collectedCountAllTime"))),
        buzz: value_u64(stats.and_then(|value| value.get("tippedAmountCountAllTime"))),
        likes: value_u64(stats.and_then(|value| value.get("likeCountAllTime"))),
        hearts: value_u64(stats.and_then(|value| value.get("heartCountAllTime"))),
    }
}

pub fn image_tags(hit: &SearchImageHit) -> Vec<String> {
    hit.tag_names
        .iter()
        .filter_map(|value| value.clone())
        .filter(|value| !value.trim().is_empty())
        .collect()
}

pub fn comfy_workflow_value(hit: &SearchImageHit) -> Option<Value> {
    let metadata = hit.metadata.as_ref()?;
    extract_comfy_workflow(metadata)
}

pub fn comfy_workflow_json(hit: &SearchImageHit) -> Option<String> {
    let workflow = comfy_workflow_value(hit)?;
    serde_json::to_string_pretty(&workflow).ok()
}

pub fn comfy_workflow_node_count(hit: &SearchImageHit) -> Option<usize> {
    let workflow = comfy_workflow_value(hit)?;
    count_workflow_nodes(&workflow)
}

fn extract_prompt_from_metadata(metadata: &Value) -> Option<String> {
    if let Some(value) = metadata.get("prompt").and_then(value_string) {
        return Some(value);
    }
    if let Some(value) = metadata.get("Prompt").and_then(value_string) {
        return Some(value);
    }
    if let Some(value) = metadata.get("positivePrompt").and_then(value_string) {
        return Some(value);
    }
    None
}

fn extract_comfy_workflow(metadata: &Value) -> Option<Value> {
    for key in ["workflow", "comfy", "nodes"] {
        if let Some(value) = metadata.get(key)
            && is_comfy_like(value)
        {
            return Some(value.clone());
        }
    }

    if let Some(value) = metadata.get("prompt")
        && is_comfy_like(value)
    {
        return Some(value.clone());
    }

    is_comfy_like(metadata).then_some(metadata.clone())
}

fn is_comfy_like(value: &Value) -> bool {
    if let Some(object) = value.as_object() {
        if object.contains_key("nodes") && object.contains_key("links") {
            return true;
        }
        if object.values().any(|node| {
            node.as_object().is_some_and(|node_obj| {
                node_obj.contains_key("class_type") || node_obj.contains_key("inputs")
            })
        }) {
            return true;
        }
    }

    if let Some(array) = value.as_array() {
        return array.iter().any(|node| {
            node.as_object().is_some_and(|node_obj| {
                node_obj.contains_key("type")
                    || node_obj.contains_key("class_type")
                    || node_obj.contains_key("inputs")
            })
        });
    }

    false
}

fn count_workflow_nodes(value: &Value) -> Option<usize> {
    if let Some(nodes) = value.get("nodes").and_then(|nodes| nodes.as_array()) {
        return Some(nodes.len());
    }
    if let Some(object) = value.as_object() {
        let count = object
            .values()
            .filter(|node| {
                node.as_object().is_some_and(|node_obj| {
                    node_obj.contains_key("class_type") || node_obj.contains_key("inputs")
                })
            })
            .count();
        if count > 0 {
            return Some(count);
        }
    }
    if let Some(array) = value.as_array() {
        return Some(array.len());
    }
    None
}

fn value_string(value: &Value) -> Option<String> {
    match value {
        Value::String(raw) if !raw.trim().is_empty() => Some(raw.clone()),
        Value::Number(raw) => Some(raw.to_string()),
        Value::Bool(raw) => Some(raw.to_string()),
        _ => None,
    }
}

fn value_u64(value: Option<&Value>) -> u64 {
    match value {
        Some(Value::Number(raw)) => raw.as_u64().unwrap_or_default(),
        Some(Value::String(raw)) => raw.parse::<u64>().unwrap_or_default(),
        _ => 0,
    }
}
