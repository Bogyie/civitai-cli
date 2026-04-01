use anyhow::Result;
use reqwest::Url;

pub fn escape_filter_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn push_equals_filters(filters: &mut Vec<String>, field: &str, values: &[String]) {
    for value in values.iter().filter(|value| !value.trim().is_empty()) {
        filters.push(format!(
            "{field} = \"{}\"",
            escape_filter_value(value.trim())
        ));
    }
}

pub fn build_created_at_filters(raw: &str, field_name: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let Some((start, end)) = trimmed.split_once('-').or_else(|| trimmed.split_once(':')) else {
        return Vec::new();
    };

    let mut filters = Vec::new();

    if let Ok(start_unix) = start.trim().parse::<i64>() {
        let normalized = if start_unix < 10_000_000_000 {
            start_unix * 1000
        } else {
            start_unix
        };
        filters.push(format!("{field_name} >= {normalized}"));
    }

    if let Ok(end_unix) = end.trim().parse::<i64>() {
        let normalized = if end_unix < 10_000_000_000 {
            end_unix * 1000
        } else {
            end_unix
        };
        filters.push(format!("{field_name} <= {normalized}"));
    }

    filters
}

pub fn append_csv_pair(acc: &mut Vec<(String, String)>, key: &str, values: &[String]) {
    if values.is_empty() {
        return;
    }
    acc.push((key.to_string(), values.join(",")));
}

pub fn split_multi(values: Option<&Vec<String>>) -> Vec<String> {
    values
        .into_iter()
        .flat_map(|items| items.iter())
        .flat_map(|item| item.split(','))
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn split_multi_keys(
    map: &std::collections::BTreeMap<String, Vec<String>>,
    keys: &[&str],
) -> Vec<String> {
    keys.iter()
        .filter_map(|key| map.get(*key))
        .flat_map(|items| items.iter())
        .flat_map(|item| item.split(','))
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn normalize_search_url(raw: &str, default_path: &str) -> Result<String> {
    if raw.contains("://") {
        return Ok(raw.to_string());
    }

    if raw.starts_with('?') {
        return Ok(format!("https://example.local{default_path}{raw}"));
    }

    if raw.starts_with('/') {
        return Ok(format!("https://example.local{raw}"));
    }

    Ok(format!("https://example.local{default_path}?{raw}"))
}

pub fn parse_query_map(url: &Url) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut map: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for (key, value) in url.query_pairs() {
        map.entry(key.to_string()).or_default().push(value.to_string());
    }
    map
}
