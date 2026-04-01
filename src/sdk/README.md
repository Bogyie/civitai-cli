# Civitai SDK

이 폴더는 Civitai 웹 검색, 공식 API 수집, 다운로드를 독립적으로 다루는 SDK 계층입니다.

현재 포함하는 기능:

- 이미지 검색 URL 파싱/생성
- Civitai 웹 검색 백엔드(`images_v6`) 호출
- 이미지 page URL / media URL helper
- 모델 검색 URL 파싱/생성
- Civitai 웹 검색 백엔드(`models_v9`) 호출
- 모델 page URL / download URL helper
- 모델 / 이미지 / 비디오용 범용 다운로드 spec / request / stream helper
- pause / resume / cancel 제어와 progress event 전송
- Civitai 공식 API(`api/v1`) 기반 모델 / 이미지 수집 helper

파일 구성:

- `mod.rs`: SDK 진입점과 public re-export
- `client.rs`: 공통 builder와 capability별 client 구현
- `constants.rs`: Civitai 관련 상수
- `shared.rs`: 공통 URL/filter/query 유틸
- `image_search.rs`: 이미지 검색 state/sort/response 타입
- `model_search.rs`: 모델 검색 state/sort/response 타입과 다운로드 helper
- `download.rs`: 범용 다운로드 spec/options/event 타입
- `api.rs`: 공식 API 타입 re-export와 URL builder

## Example

```rust
use crate::sdk::{
    DownloadOptions, ModelDownloadAuth, SdkClientBuilder, ImageSearchSortBy,
    ImageSearchState, ModelSearchSortBy, ModelSearchState,
};

async fn demo() -> anyhow::Result<()> {
    let clients = SdkClientBuilder::new().build_clients()?;

    let image_state = ImageSearchState {
        query: Some("man".to_string()),
        tags: vec!["pg".to_string()],
        sort_by: ImageSearchSortBy::MostReactions,
        limit: Some(5),
        ..Default::default()
    };
    let image_response = clients.web.search_images(&image_state).await?;
    for hit in image_response.hits.iter() {
        println!("page={}", clients.download.image_page_url(hit));
        println!(
            "media={}",
            clients.download.original_media_url(hit).unwrap_or_default()
        );
    }

    let model_state = ModelSearchState {
        query: Some("hello".to_string()),
        tags: vec!["anime".to_string()],
        sort_by: ModelSearchSortBy::MostDownloaded,
        limit: Some(5),
        ..Default::default()
    };
    let model_response = clients.web.search_models(&model_state).await?;
    for hit in model_response.hits.iter() {
        println!("model={}", clients.download.model_page_url(hit));
        println!(
            "download={}",
            clients.download.model_download_url(hit).unwrap_or_default()
        );
    }

    let request = clients.download.build_model_download_request(
        123456,
        Some(&ModelDownloadAuth::BearerToken("your-token".to_string())),
    );
    let _request = request.build()?;

    let model = clients.api.get_model(123456).await?;
    println!("api_model={}", model.name);

    if let Some(spec) = clients.download.build_model_download_spec(
        &model_response.hits[0],
        Some(ModelDownloadAuth::BearerToken("your-token".to_string())),
    ) {
        let result = clients
            .download
            .download(
                &spec,
                &DownloadOptions::to_file("./model.bin"),
                None,
                None,
            )
            .await?;
        println!("saved={}", result.path.display());
    }

    Ok(())
}
```

개별 client만 만들고 싶으면:

```rust
use crate::sdk::{DownloadClient, SdkClientBuilder};

let download = SdkClientBuilder::new()
    .media_delivery_namespace("custom-namespace")
    .build_download()?;

let _ = DownloadClient::new()?;
```

설정을 먼저 만들고 재사용하고 싶으면:

```rust
use crate::sdk::{SearchSdkConfig, WebSearchClient};

let config = SearchSdkConfig::builder()
    .meili_base_url("https://search-new.civitai.com")
    .meili_client_key("your-key")
    .media_delivery_namespace("custom-namespace")
    .build_config();

let web = WebSearchClient::with_config(config)?;
```

## Notes

- 이 SDK는 현재 Civitai 공개 웹 검색 페이지가 사용하는 검색 백엔드를 직접 호출합니다.
- 동시에 공식 API(`api/v1`) 기반 수집 메서드도 제공해서, 사용하는 쪽에서 fallback 후보로 조합할 수 있습니다.
- 기본 상수값은 유지하지만, 실제 사용 설정은 `SdkClientBuilder` 또는 각 capability client의 `with_config()`에서 주입할 수 있습니다.
- 권장 사용 방식은 단일 만능 client보다 `WebSearchClient`, `ApiClient`, `DownloadClient`를 역할별로 나눠 쓰는 구조입니다.
- 다운로드 계층은 ComfyUI 같은 외부 프레임워크 경로 규칙을 직접 알지 않습니다. 저장 위치/파일명/이벤트 연결은 SDK를 사용하는 쪽에서 주입하는 방향입니다.
- 기존 앱 코드와는 아직 느슨하게 분리되어 있어서, 나중에 adapter 레이어를 통해 연결하는 방향을 전제로 합니다.
- 인증이 필요 없는 모델에도 `token` 또는 `Authorization: Bearer <token>`을 포함해 요청할 수 있습니다.
