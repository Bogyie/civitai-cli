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
        .or_else(|| hit.prompt
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| hit.metadata.as_ref().and_then(extract_prompt_from_metadata)))
}

pub fn image_negative_prompt(hit: &SearchImageHit) -> Option<String> {
    if hit.hide_meta.unwrap_or(false) {
        return None;
    }

    generation_data(hit)
        .and_then(|data| data.meta.and_then(|meta| meta.negative_prompt))
        .or_else(|| hit.metadata
        .as_ref()
        .and_then(extract_negative_prompt_from_metadata))
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

pub fn image_used_models(hit: &SearchImageHit) -> Vec<String> {
    image_used_model_entries(hit)
        .into_iter()
        .map(|item| item.label)
        .collect()
}

pub fn image_used_model_entries(hit: &SearchImageHit) -> Vec<ParsedUsedModel> {
    let mut values = Vec::new();
    let mut seen_labels = HashSet::new();

    if let Some(base_model) = hit.base_model.as_ref().filter(|value| !value.trim().is_empty()) {
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

    if let Some(data) = generation_data(hit) {
        for resource in data.resources {
            if let Some(item) = parsed_used_model_from_generation_resource(&resource) {
                push_used_model(&mut values, &mut seen_labels, item);
            }
        }
    }

    if let Some(metadata) = hit.metadata.as_ref() {
        collect_model_artifacts(metadata, None, &mut values, &mut seen_labels);
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
    if let Some(Value::String(raw)) = metadata.get("meta")
        && let Ok(parsed) = serde_json::from_str::<Value>(raw)
        && let Some(value) = extract_prompt_from_metadata(&parsed)
    {
        return Some(value);
    }
    if let Some(Value::String(raw)) = metadata.get("comfy")
        && let Ok(parsed) = serde_json::from_str::<Value>(raw)
    {
        return extract_prompt_from_metadata(&parsed);
    }
    None
}

fn extract_negative_prompt_from_metadata(metadata: &Value) -> Option<String> {
    for key in ["negativePrompt", "negative_prompt", "negPrompt"] {
        if let Some(value) = metadata.get(key).and_then(value_string) {
            return Some(value);
        }
    }
    if let Some(value) = metadata.get("meta").and_then(extract_negative_prompt_from_metadata) {
        return Some(value);
    }
    if let Some(Value::String(raw)) = metadata.get("meta")
        && let Ok(parsed) = serde_json::from_str::<Value>(raw)
        && let Some(value) = extract_negative_prompt_from_metadata(&parsed)
    {
        return Some(value);
    }
    if let Some(Value::String(raw)) = metadata.get("comfy")
        && let Ok(parsed) = serde_json::from_str::<Value>(raw)
        && let Some(value) = extract_negative_prompt_from_metadata(&parsed)
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
                if let Some(parsed) = parse_nested_json(nested)
                    && let Some(value) = extract_negative_prompt_from_prompt_graph(&parsed)
                {
                    return Some(value);
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
        Value::String(raw) => serde_json::from_str::<Value>(raw)
            .ok()
            .and_then(|parsed| extract_negative_prompt_from_prompt_graph(&parsed)),
        _ => None,
    }
}

fn extract_comfy_workflow(metadata: &Value) -> Option<Value> {
    if let Some(comfy) = metadata.get("comfy") {
        if let Some(workflow) = comfy.get("workflow") {
            if is_comfy_like(workflow) {
                return Some(workflow.clone());
            }
            if let Some(parsed) = parse_nested_json(workflow)
                && is_comfy_like(&parsed)
            {
                return Some(parsed);
            }
        }
        if let Some(prompt) = comfy.get("prompt") {
            if is_comfy_like(prompt) {
                return Some(prompt.clone());
            }
            if let Some(parsed) = parse_nested_json(prompt)
                && is_comfy_like(&parsed)
            {
                return Some(parsed);
            }
        }
    }

    for key in ["workflow", "comfy", "nodes"] {
        if let Some(value) = metadata.get(key) {
            if is_comfy_like(value) {
                return Some(value.clone());
            }
            if let Some(parsed) = parse_nested_json(value)
                && is_comfy_like(&parsed)
            {
                return Some(parsed);
            }
        }
    }

    if let Some(value) = metadata.get("prompt")
    {
        if is_comfy_like(value) {
            return Some(value.clone());
        }
        if let Some(parsed) = parse_nested_json(value)
            && is_comfy_like(&parsed)
        {
            return Some(parsed);
        }
    }

    if let Some(value) = metadata.get("meta") {
        if let Some(found) = extract_comfy_workflow(value) {
            return Some(found);
        }
        if let Some(parsed) = parse_nested_json(value)
            && let Some(found) = extract_comfy_workflow(&parsed)
        {
            return Some(found);
        }
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

fn value_u64_opt(value: &Value) -> Option<u64> {
    match value {
        Value::Number(raw) => raw.as_u64(),
        Value::String(raw) => raw.parse::<u64>().ok(),
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

fn collect_model_artifacts(
    value: &Value,
    key_hint: Option<&str>,
    values: &mut Vec<ParsedUsedModel>,
    seen_labels: &mut HashSet<String>,
) {
    match value {
        Value::Object(map) => {
            if let Some(model_name) = map.get("modelName").and_then(value_string) {
                let model_type = map
                    .get("modelType")
                    .and_then(value_string)
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| "Model".to_string());
                if !is_supported_model_label(&model_type) {
                    for (key, nested) in map {
                        collect_model_artifacts(nested, Some(key), values, seen_labels);
                    }
                    return;
                }
                push_used_model(
                    values,
                    seen_labels,
                    ParsedUsedModel {
                        label: format!("{model_type}: {model_name}"),
                        query_name: Some(model_name),
                        model_id: map.get("modelId").and_then(value_u64_opt),
                        version_id: map.get("versionId").and_then(value_u64_opt),
                        navigable: true,
                    },
                );
                return;
            }
            if let Some(resource_name) = map.get("name").and_then(value_string) {
                let resource_type = map
                    .get("type")
                    .and_then(value_string)
                    .filter(|value| !value.trim().is_empty());
                let weight = map
                    .get("weight")
                    .and_then(value_string)
                    .filter(|value| !value.trim().is_empty());
                if let Some(resource_type) = resource_type
                    && is_supported_model_label(&resource_type)
                {
                    let mut label = format!("{}: {}", resource_type.to_uppercase(), resource_name);
                    if let Some(weight) = weight {
                        label.push_str(&format!(" x{weight}"));
                    }
                    push_used_model(
                        values,
                        seen_labels,
                        ParsedUsedModel {
                            label,
                            navigable: false,
                            ..ParsedUsedModel::default()
                        },
                    );
                }
            }
            for (key, nested) in map {
                collect_model_artifacts(nested, Some(key), values, seen_labels);
            }
        }
        Value::Array(items) => {
            for nested in items {
                collect_model_artifacts(nested, key_hint, values, seen_labels);
            }
        }
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return;
            }

            if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
                collect_model_artifacts(&parsed, key_hint, values, seen_labels);
                return;
            }

            let lower = trimmed.to_ascii_lowercase();
            let key = key_hint.unwrap_or_default().to_ascii_lowercase();
            let is_generic_model_label = matches!(
                lower.as_str(),
                "model"
                    | "checkpoint"
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
                    | "other"
            );
            let looks_like_model_file = lower.ends_with(".safetensors")
                || lower.ends_with(".ckpt")
                || lower.ends_with(".pt");
            let key_suggests_named_model = key.contains("lora")
                || key.contains("ckpt_name")
                || key.contains("checkpoint")
                || key.contains("unet")
                || key.contains("vae")
                || key.contains("clip");

            if !is_generic_model_label && (looks_like_model_file || key_suggests_named_model) {
                let label = match key.as_str() {
                    key if key.contains("lora") => "LoRA",
                    key if key.contains("vae") => "VAE",
                    key if key.contains("clip") => "CLIP",
                    key if key.contains("unet") => "UNet",
                    key if key.contains("checkpoint") || key.contains("ckpt") => "Checkpoint",
                    _ => "Model",
                };
                push_used_model(
                    values,
                    seen_labels,
                    ParsedUsedModel {
                        label: format!("{label}: {trimmed}"),
                        navigable: false,
                        ..ParsedUsedModel::default()
                    },
                );
            }
        }
        _ => {}
    }
}

fn is_supported_model_label(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "model"
            | "checkpoint"
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
    )
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


fn parse_nested_json(value: &Value) -> Option<Value> {
    match value {
        Value::String(raw) => serde_json::from_str::<Value>(raw).ok(),
        _ => None,
    }
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

                if let Some(parsed) = parse_nested_json(nested) {
                    collect_generation_info(&parsed, info);
                }
                collect_generation_info(nested, info);
            }
        }
        Value::Array(items) => {
            for nested in items {
                collect_generation_info(nested, info);
            }
        }
        Value::String(raw) => {
            if let Ok(parsed) = serde_json::from_str::<Value>(raw) {
                collect_generation_info(&parsed, info);
            }
        }
        _ => {}
    }
}
