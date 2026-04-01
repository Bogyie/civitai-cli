# Civitai Search SDK

이 폴더는 Civitai 웹 검색 로직을 독립적으로 다루는 SDK 계층입니다.

현재 포함하는 기능:

- 이미지 검색 URL 파싱/생성
- Civitai 웹 검색 백엔드(`images_v6`) 호출
- 이미지 page URL / media URL helper
- 모델 검색 URL 파싱/생성
- Civitai 웹 검색 백엔드(`models_v9`) 호출
- 모델 page URL / download URL helper

파일 구성:

- `mod.rs`: SDK 진입점과 public re-export
- `client.rs`: HTTP client와 웹 검색 호출 로직
- `constants.rs`: Civitai 관련 상수
- `shared.rs`: 공통 URL/filter/query 유틸
- `image_search.rs`: 이미지 검색 state/sort/response 타입
- `model_search.rs`: 모델 검색 state/sort/response 타입과 다운로드 helper

## Example

```rust
use crate::sdk::{
    ImageSearchSortBy, ImageSearchState, ModelDownloadAuth, ModelSearchSortBy,
    ModelSearchState, SearchSdkClient, SearchSdkConfig,
};

async fn demo() -> anyhow::Result<()> {
    let sdk = SearchSdkClient::new()?;

    let image_state = ImageSearchState {
        query: Some("man".to_string()),
        tags: vec!["pg".to_string()],
        sort_by: ImageSearchSortBy::MostReactions,
        limit: Some(5),
        ..Default::default()
    };
    let image_response = sdk.search_images_web(&image_state).await?;
    for hit in image_response.hits.iter() {
        println!("page={}", hit.image_page_url());
        println!("media={}", hit.original_media_url().unwrap_or_default());
    }

    let model_state = ModelSearchState {
        query: Some("hello".to_string()),
        tags: vec!["anime".to_string()],
        sort_by: ModelSearchSortBy::MostDownloaded,
        limit: Some(5),
        ..Default::default()
    };
    let model_response = sdk.search_models_web(&model_state).await?;
    for hit in model_response.hits.iter() {
        println!("model={}", hit.model_page_url());
        println!("download={}", hit.model_download_url().unwrap_or_default());
    }

    let request = sdk.build_model_download_request(
        123456,
        Some(&ModelDownloadAuth::BearerToken("your-token".to_string())),
    );
    let _request = request.build()?;

    Ok(())
}
```

커스텀 endpoint / namespace / key를 주입하고 싶으면:

```rust
use crate::sdk::{SearchSdkClient, SearchSdkConfig};

let sdk = SearchSdkClient::with_config(SearchSdkConfig {
    meili_base_url: "https://search-new.civitai.com".to_string(),
    meili_client_key: "your-key".to_string(),
    media_delivery_namespace: "custom-namespace".to_string(),
    ..Default::default()
})?;
```

builder 스타일로는:

```rust
use crate::sdk::{SearchSdkClient, SearchSdkConfig};

let sdk = SearchSdkClient::with_config(
    SearchSdkConfig::builder()
        .meili_base_url("https://search-new.civitai.com")
        .meili_client_key("your-key")
        .media_delivery_namespace("custom-namespace")
        .build(),
)?;
```

## Notes

- 이 SDK는 현재 Civitai 공개 웹 검색 페이지가 사용하는 검색 백엔드를 직접 호출합니다.
- 기본 상수값은 유지하지만, 실제 사용 설정은 `SearchSdkClient`가 소유하며 `with_config()`로 주입할 수 있습니다.
- 기존 앱 코드와는 아직 느슨하게 분리되어 있어서, 나중에 adapter 레이어를 통해 연결하는 방향을 전제로 합니다.
- 인증이 필요 없는 모델에도 `token` 또는 `Authorization: Bearer <token>`을 포함해 요청할 수 있습니다.
