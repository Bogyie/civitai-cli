use civitai_cli::sdk::{ImageGenerationData, SearchImageHit};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Debug, Clone, Default)]
pub struct ParsedImageStats {
    pub reactions: u64,
    pub comments: u64,
    pub collected: u64,
    pub buzz: u64,
    pub likes: u64,
    pub hearts: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ParsedGenerationInfo {
    pub cfg_scale: Option<String>,
    pub steps: Option<String>,
    pub sampler: Option<String>,
    pub seed: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ParsedUsedModel {
    pub label: String,
    pub query_name: Option<String>,
    pub model_id: Option<u64>,
    pub version_id: Option<u64>,
    pub navigable: bool,
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

    generation_data(hit)
        .and_then(|data| data.meta.and_then(|meta| meta.prompt))
        .or_else(|| {
            hit.prompt
                .clone()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| hit.metadata.as_ref().and_then(extract_prompt_from_metadata))
        })
}

pub fn image_negative_prompt(hit: &SearchImageHit) -> Option<String> {
    if hit.hide_meta.unwrap_or(false) {
        return None;
    }

    generation_data(hit)
        .and_then(|data| data.meta.and_then(|meta| meta.negative_prompt))
        .or_else(|| {
            hit.metadata
                .as_ref()
                .and_then(extract_negative_prompt_from_metadata)
        })
}

pub fn image_stats(hit: &SearchImageHit) -> ParsedImageStats {
    let stats = hit.stats.as_ref();
    ParsedImageStats {
        reactions: stats
            .map(|value| value.reaction_count_all_time)
            .unwrap_or(0),
        comments: stats.map(|value| value.comment_count_all_time).unwrap_or(0),
        collected: stats
            .map(|value| value.collected_count_all_time)
            .unwrap_or(0),
        buzz: stats
            .map(|value| value.tipped_amount_count_all_time)
            .unwrap_or(0),
        likes: stats.map(|value| value.like_count_all_time).unwrap_or(0),
        hearts: stats.map(|value| value.heart_count_all_time).unwrap_or(0),
    }
}

pub fn image_tags(hit: &SearchImageHit) -> Vec<String> {
    hit.tag_names
        .iter()
        .filter_map(|value| value.clone())
        .filter(|value| !value.trim().is_empty())
        .collect()
}

pub fn image_used_models(hit: &SearchImageHit) -> Vec<String> {
    image_used_model_entries(hit)
        .into_iter()
        .map(|item| item.label)
        .collect()
}

pub fn image_used_model_entries(hit: &SearchImageHit) -> Vec<ParsedUsedModel> {
    let mut values = Vec::new();
    let mut seen_labels = HashSet::new();

    if let Some(base_model) = hit
        .base_model
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        push_used_model(
            &mut values,
            &mut seen_labels,
            ParsedUsedModel {
                label: format!("Base Model: {base_model}"),
                navigable: false,
                ..ParsedUsedModel::default()
            },
        );
    }

    for resource in structured_generation_resources(hit) {
        if let Some(item) = parsed_used_model_from_generation_resource(&resource) {
            push_used_model(&mut values, &mut seen_labels, item);
        }
    }

    values
}

pub fn image_generation_info(hit: &SearchImageHit) -> ParsedGenerationInfo {
    let mut info = ParsedGenerationInfo::default();

    if let Some(data) = generation_data(hit)
        && let Some(meta) = data.meta
    {
        info.cfg_scale = meta.cfg_scale.map(|value| value.to_string());
        info.steps = meta.steps.map(|value| value.to_string());
        info.sampler = meta.sampler;
        info.seed = meta.seed.map(|value| value.to_string());
    }

    if let Some(metadata) = hit.metadata.as_ref() {
        collect_generation_info(metadata, &mut info);
    }

    if let Some(workflow) = comfy_workflow_value(hit) {
        collect_generation_info(&workflow, &mut info);
    }

    info
}

pub fn comfy_workflow_value(hit: &SearchImageHit) -> Option<Value> {
    if let Some(data) = generation_data(hit)
        && let Some(meta) = data.meta
        && let Some(comfy) = meta.comfy
    {
        if let Some(workflow) = comfy.workflow
            && is_comfy_like(&workflow)
        {
            return Some(workflow);
        }
        if let Some(prompt) = comfy.prompt
            && is_comfy_like(&prompt)
        {
            return Some(prompt);
        }
    }
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
    if let Some(value) = metadata.get("meta").and_then(extract_prompt_from_metadata) {
        return Some(value);
    }
    if let Some(value) = metadata.get("comfy").and_then(extract_prompt_from_metadata) {
        return Some(value);
    }
    None
}

fn extract_negative_prompt_from_metadata(metadata: &Value) -> Option<String> {
    for key in ["negativePrompt", "negative_prompt", "negPrompt"] {
        if let Some(value) = metadata.get(key).and_then(value_string) {
            return Some(value);
        }
    }
    if let Some(value) = metadata
        .get("meta")
        .and_then(extract_negative_prompt_from_metadata)
    {
        return Some(value);
    }
    if let Some(value) = metadata
        .get("comfy")
        .and_then(extract_negative_prompt_from_metadata)
    {
        return Some(value);
    }
    if let Some(value) = metadata.get("prompt") {
        return extract_negative_prompt_from_prompt_graph(value);
    }
    None
}

fn extract_negative_prompt_from_prompt_graph(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => {
            for (key, nested) in map {
                let normalized = key.to_ascii_lowercase();
                if (normalized.contains("negative") || normalized == "text_negative")
                    && let Some(text) = value_string(nested)
                {
                    return Some(text);
                }
                if let Some(value) = extract_negative_prompt_from_prompt_graph(nested) {
                    return Some(value);
                }
            }
            None
        }
        Value::Array(items) => items
            .iter()
            .find_map(extract_negative_prompt_from_prompt_graph),
        _ => None,
    }
}

fn extract_comfy_workflow(metadata: &Value) -> Option<Value> {
    if let Some(comfy) = metadata.get("comfy") {
        if let Some(workflow) = comfy.get("workflow")
            && is_comfy_like(workflow)
        {
            return Some(workflow.clone());
        }
        if let Some(prompt) = comfy.get("prompt")
            && is_comfy_like(prompt)
        {
            return Some(prompt.clone());
        }
    }

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

    if let Some(value) = metadata.get("meta")
        && let Some(found) = extract_comfy_workflow(value)
    {
        return Some(found);
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

fn generation_data(hit: &SearchImageHit) -> Option<ImageGenerationData> {
    let metadata = hit.metadata.as_ref()?;
    metadata
        .get("_generationData")
        .cloned()
        .and_then(|value| serde_json::from_value::<ImageGenerationData>(value).ok())
}

fn structured_generation_resources(
    hit: &SearchImageHit,
) -> Vec<civitai_cli::sdk::ImageGenerationResource> {
    if let Some(data) = generation_data(hit)
        && !data.resources.is_empty()
    {
        return data.resources;
    }

    hit.metadata
        .as_ref()
        .and_then(|metadata| metadata.get("resources"))
        .cloned()
        .and_then(|value| {
            serde_json::from_value::<Vec<civitai_cli::sdk::ImageGenerationResource>>(value).ok()
        })
        .unwrap_or_default()
}

fn push_used_model(
    values: &mut Vec<ParsedUsedModel>,
    seen_labels: &mut HashSet<String>,
    item: ParsedUsedModel,
) {
    let key = item.label.to_ascii_lowercase();
    if seen_labels.insert(key) {
        values.push(item);
    }
}

fn parsed_used_model_from_generation_resource(
    resource: &civitai_cli::sdk::ImageGenerationResource,
) -> Option<ParsedUsedModel> {
    let model_name = resource.model_name.as_ref()?.trim();
    let model_type = resource
        .model_type
        .as_ref()
        .filter(|value| is_supported_generation_resource_type(value))?;
    if model_name.is_empty() {
        return None;
    }

    Some(ParsedUsedModel {
        label: format!("{model_type}: {model_name}"),
        query_name: Some(model_name.to_string()),
        model_id: resource.model_id,
        version_id: resource.version_id.or(resource.model_version_id),
        navigable: true,
    })
}

fn is_supported_generation_resource_type(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "checkpoint"
            | "lora"
            | "lycoris"
            | "textualinversion"
            | "textual inversion"
            | "embedding"
            | "hypernetwork"
            | "controlnet"
            | "vae"
            | "unet"
            | "clip"
            | "model"
    )
}

fn collect_generation_info(value: &Value, info: &mut ParsedGenerationInfo) {
    match value {
        Value::Object(map) => {
            for (key, nested) in map {
                let normalized = key.to_ascii_lowercase();
                match normalized.as_str() {
                    "cfg" | "cfgscale" | "guidance" | "guidance_scale" => {
                        if info.cfg_scale.is_none() {
                            info.cfg_scale = value_string(nested);
                        }
                    }
                    "steps" => {
                        if info.steps.is_none() {
                            info.steps = value_string(nested);
                        }
                    }
                    "sampler" | "sampler_name" | "samplername" => {
                        if info.sampler.is_none() {
                            info.sampler = value_string(nested);
                        }
                    }
                    "seed" | "noise_seed" => {
                        if info.seed.is_none() {
                            info.seed = value_string(nested);
                        }
                    }
                    _ => {}
                }

                collect_generation_info(nested, info);
            }
        }
        Value::Array(items) => {
            for nested in items {
                collect_generation_info(nested, info);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use civitai_cli::sdk::SearchImageHit;
    use serde_json::json;

    #[test]
    fn reads_generation_fields_from_stringified_metadata() {
        let hit: SearchImageHit = serde_json::from_value(json!({
            "id": 1,
            "hideMeta": false,
            "metadata": {
                "_generationData": "{\"meta\":{\"prompt\":\"sunrise\",\"negativePrompt\":\"noise\",\"cfgScale\":7,\"steps\":20,\"sampler\":\"Euler\",\"seed\":42,\"comfy\":{\"workflow\":\"{\\\"nodes\\\":[{\\\"id\\\":1,\\\"type\\\":\\\"KSampler\\\"}],\\\"links\\\":[]}\"}},\"resources\":\"[{\\\"modelName\\\":\\\"Foo\\\",\\\"modelType\\\":\\\"LORA\\\",\\\"modelId\\\":99,\\\"versionId\\\":77}]\"}"
            }
        }))
        .expect("image hit");

        assert_eq!(image_prompt(&hit).as_deref(), Some("sunrise"));
        assert_eq!(image_negative_prompt(&hit).as_deref(), Some("noise"));
        assert_eq!(comfy_workflow_node_count(&hit), Some(1));
        assert_eq!(image_used_models(&hit), vec!["LORA: Foo".to_string()]);

        let info = image_generation_info(&hit);
        assert_eq!(info.cfg_scale.as_deref(), Some("7"));
        assert_eq!(info.steps.as_deref(), Some("20"));
        assert_eq!(info.sampler.as_deref(), Some("Euler"));
        assert_eq!(info.seed.as_deref(), Some("42"));
    }

    #[test]
    fn ignores_malformed_stringified_metadata_and_keeps_fallbacks() {
        let hit: SearchImageHit = serde_json::from_value(json!({
            "id": 2,
            "hideMeta": false,
            "prompt": "plain prompt",
            "metadata": {
                "meta": "{bad",
                "comfy": "[broken"
            }
        }))
        .expect("image hit");

        assert_eq!(image_prompt(&hit).as_deref(), Some("plain prompt"));
        assert_eq!(image_negative_prompt(&hit), None);
        assert_eq!(comfy_workflow_node_count(&hit), None);
        assert!(image_used_models(&hit).is_empty());
    }
}
