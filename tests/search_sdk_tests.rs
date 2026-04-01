#[path = "../src/search_sdk.rs"]
mod search_sdk;

use search_sdk::{ImageSearchState, ImageSearchSortBy, SearchImageHit, SearchSdkClient};
use serde_json::Value;

#[test]
fn parse_web_url_with_default_sorting_and_tag_filter() {
    let input = "https://civitai.com/search/images?tags=pg&sortBy=images_v6";
    let state = search_sdk::ImageSearchState::from_web_url(input).unwrap();

    assert_eq!(state.sort_by, ImageSearchSortBy::Relevance);
    assert!(state.query.is_none());
    assert_eq!(state.tags, vec!["pg".to_string()]);
}

#[test]
fn build_and_parse_round_trip() {
    let original = ImageSearchState {
        query: Some("cute cat".to_string()),
        sort_by: ImageSearchSortBy::MostReactions,
        tags: vec!["pg".to_string(), "anime".to_string()],
        users: vec!["alice".to_string()],
        tools: vec!["lora".to_string()],
        techniques: vec!["flux".to_string()],
        base_models: vec!["SDXL".to_string()],
        aspect_ratios: vec!["1:1".to_string()],
        created_at: Some("1700000000-1705000000".to_string()),
        image_id: Some(42),
        page: Some(2),
        limit: Some(20),
        extras: vec![("extra".to_string(), "value".to_string())],
        ..Default::default()
    };

    let url = original
        .to_web_url("https://civitai.com/search/images")
        .unwrap()
        .to_string();
    let parsed = search_sdk::ImageSearchState::from_web_url(&url).unwrap();

    assert_eq!(parsed, original);
}

#[test]
fn parse_multi_value_comma_and_repeat_values() {
    let input = "/search/images?tags=a,b&tags=c&users=one,two&sortBy=images_v6:createdAt:desc";
    let state = search_sdk::ImageSearchState::from_web_url(input).unwrap();

    assert_eq!(state.tags, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    assert_eq!(state.users, vec!["one".to_string(), "two".to_string()]);
    assert_eq!(state.sort_by, ImageSearchSortBy::Newest);
}

#[test]
fn has_public_metadata_check() {
    let public_meta = SearchImageHit {
        id: 1,
        url: Some("https://example.com/image.png".to_string()),
        width: Some(512),
        height: Some(512),
        r#type: Some("image".to_string()),
        created_at: Some("2026-01-01T00:00:00Z".to_string()),
        prompt: Some("a sample prompt".to_string()),
        base_model: Some("SDXL".to_string()),
        hash: Some("hash".to_string()),
        hide_meta: Some(false),
        user: None,
        stats: None,
        tag_names: vec!["pg".to_string()],
        model_version_ids: vec![],
        nsfw_level: None,
        browsing_level: None,
        sort_at: None,
        sort_at_unix: None,
        metadata: None,
        generation_process: None,
        ai_nsfw_level: None,
        combined_nsfw_level: None,
        thumbnail_url: None,
    };
    assert!(public_meta.has_public_metadata());

    let hidden_meta = SearchImageHit {
        hide_meta: Some(true),
        ..public_meta
    };
    assert!(!hidden_meta.has_public_metadata());
}

#[test]
fn builds_original_media_url_from_hit_url_token() {
    let hit = SearchImageHit {
        id: 1,
        url: Some("abc123-token".to_string()),
        width: None,
        height: None,
        r#type: Some("image".to_string()),
        created_at: None,
        prompt: None,
        base_model: None,
        hash: None,
        hide_meta: Some(false),
        user: None,
        stats: None,
        tag_names: vec![],
        model_version_ids: vec![],
        nsfw_level: None,
        browsing_level: None,
        sort_at: None,
        sort_at_unix: None,
        metadata: None,
        generation_process: None,
        ai_nsfw_level: None,
        combined_nsfw_level: None,
        thumbnail_url: None,
    };

    assert_eq!(hit.media_token(), Some("abc123-token"));
    assert_eq!(hit.image_page_url(), "https://civitai.com/images/1");
    assert_eq!(
        hit.original_media_url().as_deref(),
        Some("https://image.civitai.com/xG1nkqKTMzGDvpLrqFT7WA/abc123-token/original=true")
    );
    assert_eq!(
        hit.media_url_with_namespace("custom-namespace").as_deref(),
        Some("https://image.civitai.com/custom-namespace/abc123-token/original=true")
    );
}

#[tokio::test]
#[ignore]
async fn fetch_live_civitai_web_api_sample() -> Result<(), Box<dyn std::error::Error>> {
    let sdk = SearchSdkClient::new()?;
    let state = ImageSearchState {
        query: Some("man".to_string()),
        tags: vec!["xxx".to_string()],
        sort_by: ImageSearchSortBy::MostReactions,
        limit: Some(2),
        ..Default::default()
    };
    let typed = sdk.search_images_web(&state).await?;
    let value: Value = sdk.search_images_raw(&state).await?;

    let items = value["hits"].as_array().cloned().unwrap_or_default();
    let metadata = serde_json::json!({
        "query": value["query"].clone(),
        "processingTimeMs": value["processingTimeMs"].clone(),
        "limit": value["limit"].clone(),
        "offset": value["offset"].clone(),
        "estimatedTotalHits": value["estimatedTotalHits"].clone(),
    });

    println!("live_response_metadata = {}", metadata);
    println!("live_items_count = {}", items.len());
    println!(
        "typed_response_summary = {}",
        serde_json::json!({
            "hits": typed.hits.len(),
            "estimatedTotalHits": typed.estimated_total_hits,
            "processingTimeMs": typed.processing_time_ms,
            "limit": typed.limit,
            "offset": typed.offset,
        })
    );

    for (idx, hit) in typed.hits.iter().take(3).enumerate() {
        println!(
            "typed_item[{idx}] page_url={}, media_url={}",
            hit.image_page_url(),
            hit.original_media_url().unwrap_or_else(|| "N/A".to_string())
        );
    }

    for (idx, item) in items.iter().take(3).enumerate() {
        let id = item.get("id").and_then(Value::as_u64).unwrap_or(0);
        let username = item
            .get("user")
            .and_then(|user| user.get("username"))
            .and_then(Value::as_str)
            .unwrap_or("N/A");
        let base_model = item
            .get("baseModel")
            .and_then(Value::as_str)
            .unwrap_or("N/A");
        let prompt = item
            .get("prompt")
            .and_then(Value::as_str)
            .unwrap_or("N/A");
        let page_url = format!("https://civitai.com/images/{id}");
        let media_url = item
            .get("url")
            .and_then(Value::as_str)
            .map(|token| format!("https://image.civitai.com/xG1nkqKTMzGDvpLrqFT7WA/{token}/original=true"))
            .unwrap_or_else(|| "N/A".to_string());
        println!(
            "item[{idx}] id={id}, username={username}, baseModel={base_model}, prompt_len={}, page_url={page_url}, media_url={media_url}",
            prompt.len()
        );
    }

    assert!(!items.is_empty());
    assert_eq!(typed.hits.len(), items.len());
    for (idx, item) in items.iter().take(3).enumerate() {
        let pretty = serde_json::to_string_pretty(item).unwrap_or_else(|_| "{}".to_string());
        println!("item[{idx}] full_json = {pretty}");
        if let Some(map) = item.as_object() {
            let keys = map.keys().cloned().collect::<Vec<_>>();
            println!("item[{idx}] keys = {:?}", keys);
            for key in keys {
                let rendered = item
                    .get(&key)
                    .map(|value| serde_json::to_string(value).unwrap_or_else(|_| "null".to_string()))
                    .unwrap_or_else(|| "null".to_string());
                println!("item[{idx}].{key} = {rendered}");
            }
        }
    }
    Ok(())
}
