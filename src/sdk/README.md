# Civitai SDK

`src/sdk`는 Civitai 연동을 기존 앱 코드와 분리해서 다루기 위한 독립 SDK 계층입니다.

현재 SDK는 세 가지 capability로 나뉩니다.

- `WebSearchClient`
  Civitai 웹 검색 페이지가 실제로 사용하는 검색 백엔드로 이미지/모델을 조회합니다.
- `ApiClient`
  Civitai 공식 `api/v1` 엔드포인트로 모델/이미지 데이터를 조회합니다.
- `DownloadClient`
  모델, 이미지, 비디오 다운로드 URL 생성과 실제 스트리밍 다운로드를 담당합니다.

권장 사용 방식은 “하나의 만능 client”가 아니라, 역할별 client를 조합해서 쓰는 것입니다.

## Quick Start

```rust
use civitai_cli::sdk::{
    DownloadOptions, ImageMediaType, ImageSearchState, ModelSearchState, SdkClientBuilder,
};

async fn demo() -> anyhow::Result<()> {
    let clients = SdkClientBuilder::new().build_clients()?;

    let image_results = clients
        .web
        .search_images(&ImageSearchState {
            query: Some("landscape".to_string()),
            media_types: vec![ImageMediaType::Image],
            tags: vec!["pg".to_string()],
            limit: Some(5),
            ..Default::default()
        })
        .await?;

    let model_results = clients
        .web
        .search_models(&ModelSearchState {
            query: Some("hello".to_string()),
            tags: vec!["anime".to_string()],
            limit: Some(5),
            ..Default::default()
        })
        .await?;

    if let Some(hit) = image_results.hits.first() {
        println!("image_page={}", clients.download.image_page_url(hit));
        println!(
            "image_media={}",
            clients.download.original_media_url(hit).unwrap_or_default()
        );
    }

    if let Some(hit) = model_results.hits.first() {
        println!("model_page={}", clients.download.model_page_url(hit));
        println!(
            "model_download={}",
            clients.download.model_download_url(hit).unwrap_or_default()
        );
    }

    if let Some(hit) = model_results.hits.first() {
        if let Some(spec) = clients.download.build_model_download_spec(hit, None) {
            let result = clients
                .download
                .download(&spec, &DownloadOptions::to_file("./model.bin"), None, None)
                .await?;
            println!("saved={}", result.path.display());
        }
    }

    Ok(())
}
```

## Builder

`SearchSdkConfig::builder()` 또는 `SdkClientBuilder::new()`로 시작해서 필요한 값만 덮어쓸 수 있습니다.

```rust
use civitai_cli::sdk::SdkClientBuilder;

let clients = SdkClientBuilder::new()
    .meili_base_url("https://search-new.civitai.com")
    .meili_client_key("your-web-search-key")
    .api_base_url("https://civitai.com")
    .api_key("your-api-key")
    .media_delivery_namespace("xG1nkqKTMzGDvpLrqFT7WA")
    .build_clients()?;
```

개별 client만 만들 수도 있습니다.

```rust
use civitai_cli::sdk::{ApiClient, DownloadClient, SdkClientBuilder, WebSearchClient};

let web: WebSearchClient = SdkClientBuilder::new().build_web()?;
let api: ApiClient = SdkClientBuilder::new().api_key("token").build_api()?;
let download: DownloadClient = SdkClientBuilder::new().build_download()?;
```

설정을 재사용하고 싶으면 `build_config()` 후 `with_config()`를 쓸 수 있습니다.

```rust
use civitai_cli::sdk::{SearchSdkConfig, WebSearchClient};

let config = SearchSdkConfig::builder()
    .meili_client_key("your-web-search-key")
    .build_config();

let web = WebSearchClient::with_config(config)?;
```

## Web Search

웹 검색은 공식 API가 아니라 Civitai 검색 페이지가 쓰는 검색 백엔드를 직접 호출합니다.

- 이미지 검색: `images_v6`
- 모델 검색: `models_v9`

```rust
use civitai_cli::sdk::{
    ImageAspectRatio, ImageBaseModel, ImageMediaType, ImageSearchSortBy, ImageSearchState,
    ImageTechnique, ImageTool, WebSearchClient,
};

let web = WebSearchClient::new()?;

let response = web
    .search_images(&ImageSearchState {
        query: Some("man".to_string()),
        tags: vec!["pg".to_string()],
        media_types: vec![ImageMediaType::Image],
        tools: vec![ImageTool::ComfyUi],
        techniques: vec![ImageTechnique::Txt2Img],
        base_models: vec![ImageBaseModel::Sd15],
        aspect_ratios: vec![ImageAspectRatio::Portrait],
        sort_by: ImageSearchSortBy::MostReactions,
        limit: Some(10),
        ..Default::default()
    })
    .await?;

for hit in &response.hits {
    println!("id={}", hit.id);
    println!("page={}", hit.image_page_url());
}
```

모델 검색도 같은 방식으로 typed filter를 지원합니다.

```rust
use civitai_cli::sdk::{
    ModelBaseModel, ModelCategory, ModelCheckpointType, ModelFileFormat, ModelSearchSortBy,
    ModelSearchState, ModelType, WebSearchClient,
};

let web = WebSearchClient::new()?;

let response = web
    .search_models(&ModelSearchState {
        query: Some("hello".to_string()),
        base_models: vec![ModelBaseModel::Sd15],
        types: vec![ModelType::Checkpoint],
        checkpoint_types: vec![ModelCheckpointType::Merge],
        file_formats: vec![ModelFileFormat::SafeTensor],
        categories: vec![ModelCategory::Style],
        sort_by: ModelSearchSortBy::MostDownloaded,
        limit: Some(10),
        ..Default::default()
    })
    .await?;
```

검색 파라미터 enum은 모두 `Known + Custom(String)` 패턴입니다.

- 알려진 값은 자동완성과 가이드를 제공합니다.
- 웹에서 새 값이 생겨도 `Custom("...")`으로 그대로 round-trip 됩니다.
- 그래서 타입 안정성과 웹 호환성을 같이 가져갈 수 있습니다.

원시 JSON이 필요하면 `search_images_raw()` / `search_models_raw()`를 사용할 수 있습니다.

## Official API

공식 API는 fallback 후보나 상세 모델 데이터 수집에 적합합니다.

```rust
use civitai_cli::sdk::{ApiClient, ApiImageSearchOptions, ApiModelSearchOptions};

let api = ApiClient::new()?;

let models = api
    .search_models(&ApiModelSearchOptions {
        query: Some("hello".to_string()),
        tag: Some("anime".to_string()),
        limit: Some(10),
        ..Default::default()
    })
    .await?;

let images = api
    .search_images(&ApiImageSearchOptions {
        limit: Some(10),
        sort: Some("Most Reactions".to_string()),
        ..Default::default()
    })
    .await?;
```

추가 조회 메서드:

- `get_model(model_id)`
- `get_model_version_by_hash(hash)`
- `search_models_by_url(url)`
- `get_images_by_url(url)`

## Download

`DownloadClient`는 URL helper와 실제 다운로드 둘 다 제공합니다.

```rust
use civitai_cli::sdk::{DownloadClient, ModelDownloadAuth};

let download = DownloadClient::new()?;

let request = download.build_model_download_request(
    123456,
    Some(&ModelDownloadAuth::BearerToken("your-token".to_string())),
);

let _request = request.build()?;
```

`SearchImageHit` / `SearchModelHit`에서 바로 spec도 만들 수 있습니다.

```rust
use civitai_cli::sdk::{DownloadClient, ModelDownloadAuth};

let download = DownloadClient::new()?;

if let Some(spec) = download.build_model_download_spec(
    &model_hit,
    Some(ModelDownloadAuth::QueryToken("your-token".to_string())),
) {
    println!("url={}", spec.url);
}
```

지원 범위:

- 모델 다운로드 URL 생성
- 이미지/비디오 media URL 생성
- query token 인증
- bearer token 인증
- 스트리밍 다운로드
- pause / resume / cancel control channel
- progress event channel

참고:

- 인증이 필요 없는 모델에 `token` 또는 `Authorization: Bearer <token>`을 붙여도 동작하도록 설계했습니다.
- `DownloadClient::download()`는 전체 파일을 메모리에 적재하지 않고 chunk 단위로 저장합니다.
- `SdkClients`로 생성한 경우 내부 `reqwest::Client`는 공유됩니다.

## URL Helpers

대표 helper:

- 이미지 페이지 URL: `https://civitai.com/images/<id>`
- 모델 페이지 URL: `https://civitai.com/models/<id>`
- 모델 다운로드 URL: `https://civitai.com/api/download/models/<modelVersionId>`
- 원본 media URL: `https://image.civitai.com/<namespace>/<token>/original=true`

관련 메서드:

- `SearchImageHit::image_page_url()`
- `SearchImageHit::original_media_url()`
- `SearchModelHit::model_page_url()`
- `SearchModelHit::model_download_url()`
- `DownloadClient::image_page_url(hit)`
- `DownloadClient::original_media_url(hit)`
- `DownloadClient::model_page_url(hit)`
- `DownloadClient::model_download_url(hit)`

client helper를 쓰면 base URL, namespace, download endpoint를 config로 주입한 값이 반영됩니다.

## File Layout

- `mod.rs`
  SDK 공개 진입점과 re-export
- `client.rs`
  builder, capability client, HTTP 실행 로직
- `api.rs`
  공식 API 타입 re-export와 API URL builder
- `image_search.rs`
  이미지 검색 state, response, hit helper
- `image_search_types.rs`
  이미지 검색용 sort / media type / facet enum
- `model_search.rs`
  모델 검색 state, response, download helper
- `model_search_types.rs`
  모델 검색용 sort / base model / type / facet enum
- `download.rs`
  다운로드 spec, options, event, result, 파일명 유틸
- `shared.rs`
  공통 URL/query/filter 유틸
- `constants.rs`
  기본 endpoint, index, namespace 상수

## Live Tests

개별 라이브 테스트는 `ignored`로 분리되어 있어서 원하는 것만 실행하면 됩니다.

웹 검색:

```bash
cargo test --test search_sdk_tests fetch_live_civitai_image_web_search_sample -- --ignored --nocapture
cargo test --test search_sdk_tests fetch_live_civitai_model_web_search_sample -- --ignored --nocapture
```

공식 API:

```bash
cargo test --test sdk_api_tests fetch_live_civitai_api_model_search_sample -- --ignored --nocapture
cargo test --test sdk_api_tests fetch_live_civitai_api_image_search_sample -- --ignored --nocapture
```

## Notes

- 웹 검색과 공식 API fallback orchestration은 SDK 안에 넣지 않았습니다.
- 어떤 소스를 우선 쓸지, 실패 시 어디로 fallback할지는 SDK 사용하는 쪽에서 결정하는 방향입니다.
- 다운로드 경로 규칙이나 ComfyUI 같은 외부 프레임워크 연동도 SDK 바깥에서 주입하는 구조를 전제로 합니다.
