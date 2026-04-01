#[path = "../src/search_sdk.rs"]
mod search_sdk;

use search_sdk::{ImageSearchState, ImageSearchSortBy, SearchImageHit, SearchSdkClient};
use serde_json::Value;

use search_sdk::{
    build_model_download_url, build_model_download_url_with_token, ModelSearchSortBy,
    ModelDownloadAuth, ModelSearchState, SearchModelHit,
};

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

#[test]
fn parse_model_web_url_with_default_sorting_and_tag_filter() {
    let input = "https://civitai.com/search/models?sortBy=models_v9&query=hello&tags=anime";
    let state = ModelSearchState::from_web_url(input).unwrap();

    assert_eq!(state.sort_by, ModelSearchSortBy::Relevance);
    assert_eq!(state.query.as_deref(), Some("hello"));
    assert_eq!(state.tags, vec!["anime".to_string()]);
}

#[test]
fn build_and_parse_model_round_trip() {
    let original = ModelSearchState {
        query: Some("hello".to_string()),
        sort_by: ModelSearchSortBy::MostDownloaded,
        base_models: vec!["SDXL".to_string(), "Flux.1 D".to_string()],
        types: vec!["Checkpoint".to_string()],
        checkpoint_types: vec!["Merges".to_string()],
        file_formats: vec!["SafeTensor".to_string()],
        categories: vec!["character".to_string()],
        users: vec!["alice".to_string()],
        tags: vec!["anime".to_string(), "cute".to_string()],
        created_at: Some("1700000000-1705000000".to_string()),
        page: Some(3),
        limit: Some(24),
        extras: vec![("foo".to_string(), "bar".to_string())],
    };

    let url = original
        .to_web_url("https://civitai.com/search/models")
        .unwrap()
        .to_string();
    let parsed = ModelSearchState::from_web_url(&url).unwrap();

    assert_eq!(parsed, original);
}

#[test]
fn parse_model_multi_value_comma_and_repeat_values() {
    let input = "/search/models?baseModel=SDXL,Flux.1%20D&tags=anime,cute&tags=portrait&users=one,two&sortBy=models_v9:createdAt:desc";
    let state = ModelSearchState::from_web_url(input).unwrap();

    assert_eq!(
        state.base_models,
        vec!["SDXL".to_string(), "Flux.1 D".to_string()]
    );
    assert_eq!(
        state.tags,
        vec![
            "anime".to_string(),
            "cute".to_string(),
            "portrait".to_string()
        ]
    );
    assert_eq!(state.users, vec!["one".to_string(), "two".to_string()]);
    assert_eq!(state.sort_by, ModelSearchSortBy::Newest);
}

#[test]
fn builds_model_page_url_from_hit_id() {
    let hit = SearchModelHit {
        id: 12345,
        name: Some("Example model".to_string()),
        r#type: Some("Checkpoint".to_string()),
        created_at: None,
        last_version_at: None,
        last_version_at_unix: None,
        checkpoint_type: None,
        availability: None,
        file_formats: vec![],
        hashes: vec![],
        tags: None,
        category: None,
        permissions: None,
        metrics: None,
        rank: None,
        user: None,
        version: None,
        versions: None,
        images: None,
        can_generate: None,
        nsfw: None,
        nsfw_level: None,
    };

    assert_eq!(hit.model_page_url(), "https://civitai.com/models/12345");
}

#[test]
fn builds_model_download_urls_from_primary_version_id() {
    let hit = SearchModelHit {
        id: 12345,
        name: Some("Example model".to_string()),
        r#type: Some("Checkpoint".to_string()),
        created_at: None,
        last_version_at: None,
        last_version_at_unix: None,
        checkpoint_type: None,
        availability: None,
        file_formats: vec![],
        hashes: vec![],
        tags: None,
        category: None,
        permissions: None,
        metrics: None,
        rank: None,
        user: None,
        version: Some(serde_json::json!({
            "id": 987654,
            "baseModel": "SDXL"
        })),
        versions: Some(vec![
            serde_json::json!({ "id": 987654 }),
            serde_json::json!({ "id": 123456 })
        ]),
        images: None,
        can_generate: None,
        nsfw: None,
        nsfw_level: None,
    };

    assert_eq!(hit.primary_model_version_id(), Some(987654));
    assert_eq!(
        hit.model_download_url().as_deref(),
        Some("https://civitai.com/api/download/models/987654")
    );
    assert_eq!(
        hit.model_download_url_with_token("secret-token").as_deref(),
        Some("https://civitai.com/api/download/models/987654?token=secret-token")
    );
}

#[test]
fn builds_model_download_url_helpers() {
    assert_eq!(
        build_model_download_url(321),
        "https://civitai.com/api/download/models/321"
    );
    assert_eq!(
        build_model_download_url_with_token(321, "abc"),
        "https://civitai.com/api/download/models/321?token=abc"
    );
    assert_eq!(
        build_model_download_url_with_token(321, "   "),
        "https://civitai.com/api/download/models/321"
    );
}

#[tokio::test]
async fn builds_model_download_requests_with_optional_auth() -> Result<(), Box<dyn std::error::Error>>
{
    let sdk = SearchSdkClient::new()?;

    let plain = sdk.build_model_download_request(555, None).build()?;
    assert_eq!(
        plain.url().as_str(),
        "https://civitai.com/api/download/models/555"
    );
    assert!(plain.headers().get("authorization").is_none());

    let with_query = sdk
        .build_model_download_request(
            555,
            Some(&ModelDownloadAuth::QueryToken("abc123".to_string())),
        )
        .build()?;
    assert_eq!(
        with_query.url().as_str(),
        "https://civitai.com/api/download/models/555?token=abc123"
    );
    assert!(with_query.headers().get("authorization").is_none());

    let with_bearer = sdk
        .build_model_download_request(
            555,
            Some(&ModelDownloadAuth::BearerToken("abc123".to_string())),
        )
        .build()?;
    assert_eq!(
        with_bearer.url().as_str(),
        "https://civitai.com/api/download/models/555"
    );
    assert_eq!(
        with_bearer
            .headers()
            .get("authorization")
            .and_then(|value| value.to_str().ok()),
        Some("Bearer abc123")
    );

    Ok(())
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

#[tokio::test]
#[ignore]
async fn fetch_live_civitai_model_web_search_sample() -> Result<(), Box<dyn std::error::Error>> {
    let sdk = SearchSdkClient::new()?;
    let state = ModelSearchState {
        query: Some("hello".to_string()),
        tags: vec!["anime".to_string()],
        sort_by: ModelSearchSortBy::MostDownloaded,
        limit: Some(2),
        ..Default::default()
    };
    let typed = sdk.search_models_web(&state).await?;
    let value: Value = sdk.search_models_raw(&state).await?;

    let items = value["hits"].as_array().cloned().unwrap_or_default();
    let metadata = serde_json::json!({
        "query": value["query"].clone(),
        "processingTimeMs": value["processingTimeMs"].clone(),
        "limit": value["limit"].clone(),
        "offset": value["offset"].clone(),
        "estimatedTotalHits": value["estimatedTotalHits"].clone(),
    });

    println!("live_model_response_metadata = {}", metadata);
    println!("live_model_items_count = {}", items.len());
    println!(
        "typed_model_response_summary = {}",
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
            "typed_model_item[{idx}] page_url={}, name={}, type={}",
            hit.model_page_url(),
            hit.name.as_deref().unwrap_or("N/A"),
            hit.r#type.as_deref().unwrap_or("N/A")
        );
    }

    for (idx, item) in items.iter().take(3).enumerate() {
        let id = item.get("id").and_then(Value::as_u64).unwrap_or(0);
        let name = item.get("name").and_then(Value::as_str).unwrap_or("N/A");
        let item_type = item.get("type").and_then(Value::as_str).unwrap_or("N/A");
        let base_model = item
            .get("version")
            .and_then(|version| version.get("baseModel"))
            .and_then(Value::as_str)
            .unwrap_or("N/A");
        let checkpoint_type = item
            .get("checkpointType")
            .and_then(Value::as_str)
            .unwrap_or("N/A");
        let page_url = format!("https://civitai.com/models/{id}");
        println!(
            "model_item[{idx}] id={id}, name={name}, type={item_type}, baseModel={base_model}, checkpointType={checkpoint_type}, page_url={page_url}"
        );
    }

    assert!(!items.is_empty());
    assert_eq!(typed.hits.len(), items.len());
    for (idx, item) in items.iter().take(3).enumerate() {
        let pretty = serde_json::to_string_pretty(item).unwrap_or_else(|_| "{}".to_string());
        println!("model_item[{idx}] full_json = {pretty}");
        if let Some(map) = item.as_object() {
            let keys = map.keys().cloned().collect::<Vec<_>>();
            println!("model_item[{idx}] keys = {:?}", keys);
            for key in keys {
                let rendered = item
                    .get(&key)
                    .map(|value| serde_json::to_string(value).unwrap_or_else(|_| "null".to_string()))
                    .unwrap_or_else(|| "null".to_string());
                println!("model_item[{idx}].{key} = {rendered}");
            }
        }
    }
    Ok(())
}
