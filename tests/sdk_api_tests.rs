use civitai_cli::sdk::{
    build_api_images_search_url, build_api_model_url, build_api_model_version_by_hash_url,
    build_api_models_search_url, ApiClient, ApiImageResponse, ApiImageSearchOptions, ApiModel,
    ApiModelSearchOptions, ApiModelVersion, SearchSdkConfig,
};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn spawn_api_server() -> std::io::Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let Ok(size) = socket.read(&mut buf).await else {
                    return;
                };
                if size == 0 {
                    return;
                }

                let req = String::from_utf8_lossy(&buf[..size]);
                let req_lower = req.to_ascii_lowercase();
                let path = req
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");
                let authorized = req_lower.contains("authorization: bearer api-secret");

                let (status, body) = match path {
                    "/api/v1/models/42" if authorized => (
                        "200 OK",
                        serde_json::json!({
                            "id": 42,
                            "name": "Test Model",
                            "type": "Checkpoint",
                            "tags": [],
                            "modelVersions": []
                        })
                        .to_string(),
                    ),
                    "/api/v1/model-versions/by-hash/hash123" if authorized => (
                        "200 OK",
                        serde_json::json!({
                            "id": 55,
                            "modelId": 42,
                            "name": "Version One",
                            "baseModel": "SDXL",
                            "images": [],
                            "files": []
                        })
                        .to_string(),
                    ),
                    p if p.starts_with("/api/v1/models?") && authorized => (
                        "200 OK",
                        serde_json::json!({
                            "items": [{
                                "id": 42,
                                "name": "Test Model",
                                "type": "Checkpoint",
                                "tags": [],
                                "modelVersions": []
                            }],
                            "metadata": {
                                "totalItems": "1",
                                "currentPage": "1",
                                "pageSize": "20",
                                "totalPages": "1"
                            }
                        })
                        .to_string(),
                    ),
                    p if p.starts_with("/api/v1/images?") && authorized => (
                        "200 OK",
                        serde_json::json!({
                            "items": [{
                                "id": 99,
                                "url": "image-token",
                                "type": "image",
                                "modelVersionIds": [55]
                            }],
                            "metadata": {
                                "totalItems": "1",
                                "currentPage": "1",
                                "pageSize": "20",
                                "totalPages": "1"
                            }
                        })
                        .to_string(),
                    ),
                    _ => ("404 Not Found", "{\"error\":\"not found\"}".to_string()),
                };

                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
            });
        }
    });

    Ok(format!("http://{}", addr))
}

#[test]
fn builds_api_urls() {
    assert_eq!(
        build_api_model_url("https://civitai.com", 42),
        "https://civitai.com/api/v1/models/42"
    );
    assert_eq!(
        build_api_model_version_by_hash_url("https://civitai.com", "hash123"),
        "https://civitai.com/api/v1/model-versions/by-hash/hash123"
    );

    let models_url = build_api_models_search_url(
        "https://civitai.com",
        &ApiModelSearchOptions {
            query: Some("hello world".to_string()),
            limit: Some(50),
            tag: Some("anime".to_string()),
            base_models: Some("SDXL".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    let models_url = models_url.to_string();
    assert!(models_url.contains("/api/v1/models?"));
    assert!(models_url.contains("limit=50"));
    assert!(models_url.contains("query=hello+world") || models_url.contains("query=hello%20world"));
    assert!(models_url.contains("tag=anime"));
    assert!(models_url.contains("baseModels=SDXL"));

    let images_url = build_api_images_search_url(
        "https://civitai.com",
        &ApiImageSearchOptions {
            limit: Some(25),
            sort: Some("Most Reactions".to_string()),
            model_version_id: Some(55),
            tags: Some(77),
            ..Default::default()
        },
    )
    .unwrap();
    let images_url = images_url.to_string();
    assert!(images_url.contains("/api/v1/images?"));
    assert!(images_url.contains("limit=25"));
    assert!(images_url.contains("sort=Most+Reactions") || images_url.contains("sort=Most%20Reactions"));
    assert!(images_url.contains("modelVersionId=55"));
    assert!(images_url.contains("tags=77"));
}

#[tokio::test]
async fn fetches_api_resources_through_sdk_client() -> Result<(), Box<dyn std::error::Error>> {
    let base_url = spawn_api_server().await?;
    let sdk = ApiClient::with_config(
        SearchSdkConfig::builder()
            .api_base_url(&base_url)
            .api_key("api-secret")
            .build_config(),
    )?;

    let model: ApiModel = sdk.get_model(42).await?;
    assert_eq!(model.id, 42);
    assert_eq!(model.name, "Test Model");

    let version: ApiModelVersion = sdk.get_model_version_by_hash("hash123").await?;
    assert_eq!(version.id, 55);
    assert_eq!(version.model_id, Some(42));

    let models = sdk
        .search_models(&ApiModelSearchOptions {
            query: Some("hello".to_string()),
            limit: Some(20),
            ..Default::default()
        })
        .await?;
    assert_eq!(models.items.len(), 1);
    assert_eq!(models.items[0].id, 42);

    let images: ApiImageResponse = sdk
        .search_images(&ApiImageSearchOptions {
            limit: Some(20),
            ..Default::default()
        })
        .await?;
    assert_eq!(images.items.len(), 1);
    assert_eq!(images.items[0].id, 99);

    Ok(())
}

#[tokio::test]
#[ignore]
async fn fetch_live_civitai_api_model_search_sample() -> Result<(), Box<dyn std::error::Error>> {
    let sdk = ApiClient::new()?;
    let response = sdk
        .search_models(&ApiModelSearchOptions {
            query: Some("hello".to_string()),
            tag: Some("anime".to_string()),
            limit: Some(3),
            ..Default::default()
        })
        .await?;
    let metadata = response.metadata.clone();

    println!(
        "live_api_model_response_metadata = {}",
        json!({
            "items": response.items.len(),
            "currentPage": metadata.as_ref().and_then(|m| m.current_page.clone()),
            "pageSize": metadata.as_ref().and_then(|m| m.page_size.clone()),
            "totalItems": metadata.as_ref().and_then(|m| m.total_items.clone()),
            "totalPages": metadata.as_ref().and_then(|m| m.total_pages.clone()),
        })
    );

    for (idx, item) in response.items.iter().take(3).enumerate() {
        let version_count = item.model_versions.len();
        let tag_count = item.tags.len();
        println!(
            "api_model_item[{idx}] id={}, name={}, type={}, nsfw={}, versions={}, tags={}",
            item.id,
            item.name,
            item.r#type,
            item.nsfw,
            version_count,
            tag_count
        );

        if let Some(version) = item.model_versions.first() {
            println!(
                "api_model_item[{idx}] first_version id={}, name={}, baseModel={}, files={}",
                version.id,
                version.name,
                version.base_model,
                version.files.len()
            );
        }
    }

    assert!(!response.items.is_empty());
    Ok(())
}

#[tokio::test]
#[ignore]
async fn fetch_live_civitai_api_image_search_sample() -> Result<(), Box<dyn std::error::Error>> {
    let sdk = ApiClient::new()?;
    let response: ApiImageResponse = sdk
        .search_images(&ApiImageSearchOptions {
            limit: Some(3),
            sort: Some("Most Reactions".to_string()),
            ..Default::default()
        })
        .await?;
    let metadata = response.metadata.clone();

    println!(
        "live_api_image_response_metadata = {}",
        json!({
            "items": response.items.len(),
            "currentPage": metadata.as_ref().and_then(|m| m.current_page.clone()),
            "pageSize": metadata.as_ref().and_then(|m| m.page_size.clone()),
            "totalItems": metadata.as_ref().and_then(|m| m.total_items.clone()),
            "totalPages": metadata.as_ref().and_then(|m| m.total_pages.clone()),
        })
    );

    for (idx, item) in response.items.iter().take(3).enumerate() {
        println!(
            "api_image_item[{idx}] id={}, type={}, url={}, width={:?}, height={:?}, modelVersionIds={:?}",
            item.id,
            item.r#type.as_deref().unwrap_or("N/A"),
            item.url,
            item.width,
            item.height,
            item.model_version_ids
        );
    }

    assert!(!response.items.is_empty());
    Ok(())
}
