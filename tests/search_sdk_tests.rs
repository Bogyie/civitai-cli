use civitai_cli::sdk::{
    ApiModel, ApiModelFile, ApiModelStats, ApiModelTag, ApiModelVersion, DownloadClient,
    ImageAspectRatio, ImageBaseModel, ImageMediaType, ImageSearchSortBy, ImageSearchState,
    ImageTechnique, ImageTool, ModelBaseModel, ModelCategory, ModelCheckpointType,
    ModelDownloadAuth, ModelFileFormat, ModelSearchSortBy, ModelSearchState, ModelType,
    SearchImageHit, SearchModelHit, SearchModelVersion, SearchSdkConfig, WebSearchClient,
    build_model_download_url, build_model_download_url_with_base,
    build_model_download_url_with_token, build_model_download_url_with_token_and_base,
};
use serde_json::{Value, json};

mod fixtures {
    use super::*;

    pub fn sample_image_hit() -> SearchImageHit {
        SearchImageHit {
            id: 1,
            url: Some("abc123-token".to_string()),
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
            tag_names: vec![Some("pg".to_string())],
            model_version_ids: vec![101, 202],
            nsfw_level: None,
            browsing_level: None,
            sort_at: None,
            sort_at_unix: None,
            metadata: None,
            generation_process: None,
            ai_nsfw_level: None,
            combined_nsfw_level: None,
            thumbnail_url: None,
        }
    }

    pub fn sample_model_hit() -> SearchModelHit {
        SearchModelHit {
            id: 12345,
            name: Some("Example model".to_string()),
            r#type: Some("Checkpoint".to_string()),
            created_at: None,
            last_version_at: None,
            last_version_at_unix: None,
            checkpoint_type: Some("Merge".to_string()),
            availability: Some("Public".to_string()),
            file_formats: vec!["SafeTensor".to_string()],
            hashes: vec!["hash1".to_string()],
            tags: vec![],
            category: None,
            permissions: None,
            metrics: None,
            rank: None,
            user: None,
            version: Some(SearchModelVersion {
                id: 987654,
                base_model: Some("SDXL".to_string()),
                ..SearchModelVersion::default()
            }),
            versions: vec![
                SearchModelVersion {
                    id: 987654,
                    name: Some("primary".to_string()),
                    ..SearchModelVersion::default()
                },
                SearchModelVersion {
                    id: 123456,
                    name: Some("secondary".to_string()),
                    ..SearchModelVersion::default()
                },
            ],
            images: vec![],
            can_generate: Some(false),
            nsfw: Some(false),
            nsfw_level: None,
            extras: json!({}),
        }
    }

    pub fn sample_image_state() -> ImageSearchState {
        ImageSearchState {
            query: Some("cute cat".to_string()),
            sort_by: ImageSearchSortBy::MostReactions,
            media_types: vec![ImageMediaType::Image, ImageMediaType::Video],
            tags: vec!["pg".to_string(), "anime".to_string()],
            excluded_tags: vec!["adult".to_string()],
            users: vec!["alice".to_string()],
            tools: vec![ImageTool::ComfyUi, ImageTool::custom("lora")],
            techniques: vec![ImageTechnique::Txt2Img, ImageTechnique::custom("flux")],
            base_models: vec![ImageBaseModel::Sdxl10, ImageBaseModel::custom("SDXL")],
            aspect_ratios: vec![ImageAspectRatio::Square],
            created_at: Some("1700000000-1705000000".to_string()),
            image_id: Some(42),
            page: Some(2),
            limit: Some(20),
            extras: vec![("extra".to_string(), "value".to_string())],
        }
    }

    pub fn sample_model_state() -> ModelSearchState {
        ModelSearchState {
            query: Some("hello".to_string()),
            sort_by: ModelSearchSortBy::MostDownloaded,
            base_models: vec![ModelBaseModel::Sdxl10, ModelBaseModel::Flux1D],
            types: vec![ModelType::Checkpoint],
            checkpoint_types: vec![
                ModelCheckpointType::Merge,
                ModelCheckpointType::custom("Merges"),
            ],
            file_formats: vec![ModelFileFormat::SafeTensor],
            categories: vec![ModelCategory::Character],
            users: vec!["alice".to_string()],
            tags: vec!["anime".to_string(), "cute".to_string()],
            created_at: Some("1700000000-1705000000".to_string()),
            page: Some(3),
            limit: Some(24),
            extras: vec![("foo".to_string(), "bar".to_string())],
        }
    }

    pub fn custom_config() -> SearchSdkConfig {
        SearchSdkConfig::builder()
            .meili_base_url("https://search.civitai.test")
            .meili_client_key("secret-key")
            .civitai_web_url("https://alt.civitai.test")
            .media_delivery_url("https://media.civitai.test")
            .media_delivery_namespace("custom-space")
            .model_download_api_url("https://download.civitai.test/models")
            .images_index("images_custom")
            .models_index("models_custom")
            .user_agent("sdk-test/1.0")
            .build_config()
    }
}

mod image_state_tests {
    use super::*;

    #[test]
    fn parses_default_image_search_url() {
        let input = "https://civitai.com/search/images?tags=pg&sortBy=images_v6";
        let state = ImageSearchState::from_web_url(input).unwrap();

        assert_eq!(state.sort_by, ImageSearchSortBy::Relevance);
        assert_eq!(state.query, None);
        assert!(state.media_types.is_empty());
        assert_eq!(state.tags, vec!["pg".to_string()]);
    }

    #[test]
    fn round_trips_image_search_state() {
        let original = fixtures::sample_image_state();
        let url = original
            .to_web_url("https://civitai.com/search/images")
            .unwrap()
            .to_string();

        let parsed = ImageSearchState::from_web_url(&url).unwrap();

        assert_eq!(parsed, original);
    }

    #[test]
    fn parses_image_multi_value_query_parameters() {
        let input = "/search/images?type=image,video&type=audio&tools=ComfyUI,KREA&techniques=txt2img,workflow&baseModel=SDXL%201.0,Flux.1%20D&aspectRatio=Square,Landscape&tags=a,b&tags=c&users=one,two&sortBy=images_v6:createdAt:desc";
        let state = ImageSearchState::from_web_url(input).unwrap();

        assert_eq!(
            state.media_types,
            vec![
                ImageMediaType::Image,
                ImageMediaType::Video,
                ImageMediaType::Audio
            ]
        );
        assert_eq!(state.tools, vec![ImageTool::ComfyUi, ImageTool::Krea]);
        assert_eq!(
            state.techniques,
            vec![ImageTechnique::Txt2Img, ImageTechnique::Workflow]
        );
        assert_eq!(
            state.base_models,
            vec![ImageBaseModel::Sdxl10, ImageBaseModel::Flux1D]
        );
        assert_eq!(
            state.aspect_ratios,
            vec![ImageAspectRatio::Square, ImageAspectRatio::Landscape]
        );
        assert_eq!(state.tags, vec!["a", "b", "c"]);
        assert_eq!(state.users, vec!["one", "two"]);
        assert_eq!(state.sort_by, ImageSearchSortBy::Newest);
    }

    #[test]
    fn preserves_unknown_image_filter_values() {
        let state = ImageSearchState::from_web_url(
            "/search/images?type=panorama&tools=MyTool&techniques=my-technique&baseModel=MyBaseModel&aspectRatio=UltraWide&sortBy=images_v6:customMetric:desc",
        )
        .unwrap();

        assert_eq!(state.media_types, vec![ImageMediaType::custom("panorama")]);
        assert_eq!(state.tools, vec![ImageTool::custom("MyTool")]);
        assert_eq!(
            state.techniques,
            vec![ImageTechnique::custom("my-technique")]
        );
        assert_eq!(
            state.base_models,
            vec![ImageBaseModel::custom("MyBaseModel")]
        );
        assert_eq!(
            state.aspect_ratios,
            vec![ImageAspectRatio::custom("UltraWide")]
        );
        assert_eq!(
            state.sort_by,
            ImageSearchSortBy::Custom("images_v6:customMetric:desc".to_string())
        );

        let round_trip = state
            .to_web_url("https://civitai.com/search/images")
            .unwrap()
            .to_string();
        assert!(round_trip.contains("type=panorama"));
        assert!(round_trip.contains("tools=MyTool"));
        assert!(round_trip.contains("techniques=my-technique"));
        assert!(round_trip.contains("baseModel=MyBaseModel"));
        assert!(round_trip.contains("aspectRatio=UltraWide"));
        assert!(round_trip.contains("sortBy=images_v6%3AcustomMetric%3Adesc"));
    }

    #[test]
    fn preserves_unknown_image_query_parameters() {
        let state = ImageSearchState::from_web_url(
            "/search/images?tags=pg&foo=bar&foo=baz&sortBy=images_v6",
        )
        .unwrap();

        assert_eq!(
            state.extras,
            vec![
                ("foo".to_string(), "bar".to_string()),
                ("foo".to_string(), "baz".to_string())
            ]
        );
    }
}

mod image_hit_tests {
    use super::*;

    #[test]
    fn detects_public_metadata_only_when_visible_and_present() {
        let public_hit = fixtures::sample_image_hit();
        assert!(public_hit.has_public_metadata());

        let hidden_hit = SearchImageHit {
            hide_meta: Some(true),
            ..fixtures::sample_image_hit()
        };
        assert!(!hidden_hit.has_public_metadata());

        let promptless_hit = SearchImageHit {
            prompt: None,
            ..fixtures::sample_image_hit()
        };
        assert!(!promptless_hit.has_public_metadata());
    }

    #[test]
    fn builds_image_page_and_media_urls_with_default_constants() {
        let hit = fixtures::sample_image_hit();

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
    fn builds_image_page_and_media_urls_with_custom_base() {
        let hit = fixtures::sample_image_hit();

        assert_eq!(
            hit.image_page_url_with_base("https://alt.civitai.test/"),
            "https://alt.civitai.test/images/1"
        );
        assert_eq!(
            hit.media_url_with_base_and_namespace("https://media.civitai.test/", "custom-space")
                .as_deref(),
            Some("https://media.civitai.test/custom-space/abc123-token/original=true")
        );
    }

    #[test]
    fn returns_none_for_invalid_media_namespace() {
        let hit = fixtures::sample_image_hit();

        assert_eq!(hit.media_url_with_namespace("   "), None);
        assert_eq!(
            SearchImageHit {
                url: None,
                ..fixtures::sample_image_hit()
            }
            .original_media_url(),
            None
        );
    }
}

mod model_state_tests {
    use super::*;

    #[test]
    fn parses_default_model_search_url() {
        let input = "https://civitai.com/search/models?sortBy=models_v9&query=hello&tags=anime";
        let state = ModelSearchState::from_web_url(input).unwrap();

        assert_eq!(state.sort_by, ModelSearchSortBy::Relevance);
        assert_eq!(state.query.as_deref(), Some("hello"));
        assert_eq!(state.tags, vec!["anime".to_string()]);
    }

    #[test]
    fn round_trips_model_search_state() {
        let original = fixtures::sample_model_state();
        let url = original
            .to_web_url("https://civitai.com/search/models")
            .unwrap()
            .to_string();

        let parsed = ModelSearchState::from_web_url(&url).unwrap();

        assert_eq!(parsed, original);
    }

    #[test]
    fn parses_model_multi_value_query_parameters() {
        let input = "/search/models?baseModel=SDXL%201.0,Flux.1%20D&type=Checkpoint,LORA&checkpointType=Merge,Pruned&fileFormats=SafeTensor,GGUF&category=character,style&tags=anime,cute&tags=portrait&users=one,two&sortBy=models_v9:createdAt:desc";
        let state = ModelSearchState::from_web_url(input).unwrap();

        assert_eq!(
            state.base_models,
            vec![ModelBaseModel::Sdxl10, ModelBaseModel::Flux1D]
        );
        assert_eq!(state.types, vec![ModelType::Checkpoint, ModelType::Lora]);
        assert_eq!(
            state.checkpoint_types,
            vec![ModelCheckpointType::Merge, ModelCheckpointType::Pruned]
        );
        assert_eq!(
            state.file_formats,
            vec![ModelFileFormat::SafeTensor, ModelFileFormat::GGUF]
        );
        assert_eq!(
            state.categories,
            vec![ModelCategory::Character, ModelCategory::Style]
        );
        assert_eq!(state.tags, vec!["anime", "cute", "portrait"]);
        assert_eq!(state.users, vec!["one", "two"]);
        assert_eq!(state.sort_by, ModelSearchSortBy::Newest);
    }

    #[test]
    fn preserves_unknown_model_filter_values() {
        let state = ModelSearchState::from_web_url(
            "/search/models?baseModel=MyBaseModel&type=MyType&checkpointType=MyCheckpoint&fileFormats=MyFormat&category=my-category&sortBy=models_v9:customMetric:desc",
        )
        .unwrap();

        assert_eq!(
            state.base_models,
            vec![ModelBaseModel::custom("MyBaseModel")]
        );
        assert_eq!(state.types, vec![ModelType::custom("MyType")]);
        assert_eq!(
            state.checkpoint_types,
            vec![ModelCheckpointType::custom("MyCheckpoint")]
        );
        assert_eq!(
            state.file_formats,
            vec![ModelFileFormat::custom("MyFormat")]
        );
        assert_eq!(state.categories, vec![ModelCategory::custom("my-category")]);
        assert_eq!(
            state.sort_by,
            ModelSearchSortBy::Custom("models_v9:customMetric:desc".to_string())
        );

        let round_trip = state
            .to_web_url("https://civitai.com/search/models")
            .unwrap()
            .to_string();
        assert!(round_trip.contains("baseModel=MyBaseModel"));
        assert!(round_trip.contains("type=MyType"));
        assert!(round_trip.contains("checkpointType=MyCheckpoint"));
        assert!(round_trip.contains("fileFormats=MyFormat"));
        assert!(round_trip.contains("category=my-category"));
        assert!(round_trip.contains("sortBy=models_v9%3AcustomMetric%3Adesc"));
    }

    #[test]
    fn preserves_unknown_model_query_parameters() {
        let state = ModelSearchState::from_web_url(
            "/search/models?tags=anime&foo=bar&foo=baz&sortBy=models_v9",
        )
        .unwrap();

        assert_eq!(
            state.extras,
            vec![
                ("foo".to_string(), "bar".to_string()),
                ("foo".to_string(), "baz".to_string())
            ]
        );
    }
}

mod model_hit_tests {
    use super::*;

    #[test]
    fn builds_model_page_url_with_default_and_custom_base() {
        let hit = fixtures::sample_model_hit();

        assert_eq!(hit.model_page_url(), "https://civitai.com/models/12345");
        assert_eq!(
            hit.model_page_url_with_base("https://alt.civitai.test/"),
            "https://alt.civitai.test/models/12345"
        );
    }

    #[test]
    fn extracts_primary_model_version_id() {
        let hit = fixtures::sample_model_hit();
        assert_eq!(hit.primary_model_version_id(), Some(987654));

        let fallback_hit = SearchModelHit {
            version: None,
            versions: vec![SearchModelVersion {
                id: 123456,
                ..SearchModelVersion::default()
            }],
            ..fixtures::sample_model_hit()
        };
        assert_eq!(fallback_hit.primary_model_version_id(), Some(123456));

        let missing_hit = SearchModelHit {
            version: None,
            versions: vec![],
            ..fixtures::sample_model_hit()
        };
        assert_eq!(missing_hit.primary_model_version_id(), None);
    }

    #[test]
    fn deserializes_typed_model_search_fields() {
        let hit: SearchModelHit = serde_json::from_value(json!({
            "id": 999,
            "name": "Typed example",
            "tags": [{ "name": "portrait" }, "anime"],
            "category": { "name": "character" },
            "metrics": {
                "downloadCount": "12",
                "thumbsUpCount": 3,
                "rating": "4.5"
            },
            "version": {
                "id": "111",
                "baseModel": "SDXL",
                "files": [{
                    "id": 1,
                    "name": "weights.safetensors",
                    "type": "Model",
                    "sizeBytes": "1048576",
                    "metadata": { "format": "SafeTensor", "fp": "fp16" },
                    "primary": "true"
                }]
            },
            "versions": [{
                "id": 222,
                "downloadableFiles": [{
                    "name": "extra.bin",
                    "sizeMB": 2
                }]
            }],
            "images": [{
                "id": 7,
                "modelVersionId": "111",
                "url": "abc-token",
                "nsfw": false
            }]
        }))
        .expect("typed model hit");

        assert_eq!(hit.tags.len(), 2);
        assert_eq!(
            hit.category.as_ref().and_then(|value| value.name()),
            Some("character")
        );
        assert_eq!(
            hit.metrics.as_ref().map(|value| value.download_count),
            Some(12)
        );
        assert_eq!(hit.version.as_ref().map(|value| value.id), Some(111));
        assert_eq!(
            hit.version
                .as_ref()
                .and_then(|value| value.files.first())
                .and_then(|file| file.size_kb),
            Some(1024.0)
        );
        assert_eq!(hit.versions[0].files[0].size_kb, Some(2048.0));
        assert_eq!(hit.images[0].model_version_id, Some(111));
        assert_eq!(hit.images[0].nsfw.as_deref(), Some("false"));
    }

    #[test]
    fn converts_api_model_versions_with_file_sizes() {
        let hit: SearchModelHit = ApiModel {
            id: 77,
            name: "Detailed model".to_string(),
            description: Some("from api".to_string()),
            r#type: "Checkpoint".to_string(),
            nsfw: false,
            tags: vec![ApiModelTag::NameOnly("anime".to_string())],
            stats: Some(ApiModelStats {
                download_count: 5,
                thumbs_up_count: 3,
                thumbs_down_count: 0,
                favorite_count: 2,
                comment_count: 1,
                comment_count_all: 1,
                rating_count: 1,
                rating: 4.0,
                comment_count_weekly: 0,
            }),
            mode: None,
            creator: None,
            allow_no_credit: None,
            allow_commercial_use: None,
            allow_derivatives: None,
            allow_different_license: None,
            supports_generation: Some(false),
            poi: None,
            updated_at: Some("2026-01-01T00:00:00Z".to_string()),
            model_versions: vec![ApiModelVersion {
                id: 9001,
                model_id: Some(77),
                name: "v1".to_string(),
                base_model: "SDXL".to_string(),
                download_url: None,
                created_at: None,
                updated_at: None,
                early_access_time_frame: None,
                trained_words: None,
                description: None,
                stats: None,
                images: vec![],
                files: vec![ApiModelFile {
                    id: 10,
                    name: "weights.safetensors".to_string(),
                    file_type: Some("Model".to_string()),
                    primary: true,
                    size_kb: 3145728.0,
                    metadata: None,
                    hashes: None,
                    pickle_scan_result: None,
                    pickle_scan_message: None,
                    virus_scan_result: None,
                    scanned_at: None,
                    download_url: "https://example.test/file".to_string(),
                }],
            }],
        }
        .into();

        assert_eq!(hit.version.as_ref().map(|value| value.id), Some(9001));
        assert_eq!(hit.version.as_ref().map(|value| value.files.len()), Some(1));
        assert_eq!(
            hit.version
                .as_ref()
                .and_then(|value| value.files.first())
                .and_then(|file| file.size_kb),
            Some(3145728.0)
        );
    }

    #[test]
    fn builds_model_download_urls_from_hit() {
        let hit = fixtures::sample_model_hit();

        assert_eq!(
            hit.model_download_url().as_deref(),
            Some("https://civitai.com/api/download/models/987654")
        );
        assert_eq!(
            hit.model_download_url_with_token("secret-token").as_deref(),
            Some("https://civitai.com/api/download/models/987654?token=secret-token")
        );
        assert_eq!(
            hit.model_download_url_with_base("https://download.civitai.test/models/")
                .as_deref(),
            Some("https://download.civitai.test/models/987654")
        );
        assert_eq!(
            hit.model_download_url_with_token_and_base(
                "https://download.civitai.test/models/",
                "secret-token"
            )
            .as_deref(),
            Some("https://download.civitai.test/models/987654?token=secret-token")
        );
    }
}

mod model_download_tests {
    use super::*;

    #[test]
    fn builds_model_download_url_helpers() {
        assert_eq!(
            build_model_download_url(321),
            "https://civitai.com/api/download/models/321"
        );
        assert_eq!(
            build_model_download_url_with_base("https://download.civitai.test/models/", 321),
            "https://download.civitai.test/models/321"
        );
        assert_eq!(
            build_model_download_url_with_token(321, "abc"),
            "https://civitai.com/api/download/models/321?token=abc"
        );
        assert_eq!(
            build_model_download_url_with_token_and_base(
                "https://download.civitai.test/models/",
                321,
                "abc"
            ),
            "https://download.civitai.test/models/321?token=abc"
        );
        assert_eq!(
            build_model_download_url_with_token(321, "   "),
            "https://civitai.com/api/download/models/321"
        );
    }

    #[tokio::test]
    async fn builds_model_download_requests_with_optional_auth()
    -> Result<(), Box<dyn std::error::Error>> {
        let sdk = DownloadClient::new()?;

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
}

mod client_config_tests {
    use super::*;

    #[test]
    fn builder_overrides_only_requested_fields() {
        let config = SearchSdkConfig::builder()
            .meili_base_url("https://search.civitai.test")
            .meili_client_key("secret-key")
            .civitai_web_url("https://alt.civitai.test")
            .media_delivery_url("https://media.civitai.test")
            .media_delivery_namespace("custom-space")
            .model_download_api_url("https://download.civitai.test/models")
            .images_index("images_custom")
            .models_index("models_custom")
            .user_agent("sdk-test/1.0")
            .build_config();

        assert_eq!(config.meili_base_url, "https://search.civitai.test");
        assert_eq!(config.meili_client_key, "secret-key");
        assert_eq!(config.civitai_web_url, "https://alt.civitai.test");
        assert_eq!(config.media_delivery_url, "https://media.civitai.test");
        assert_eq!(config.media_delivery_namespace, "custom-space");
        assert_eq!(
            config.model_download_api_url,
            "https://download.civitai.test/models"
        );
        assert_eq!(config.images_index, "images_custom");
        assert_eq!(config.models_index, "models_custom");
        assert_eq!(config.user_agent, "sdk-test/1.0");
    }

    #[test]
    fn client_uses_injected_config_for_helper_urls() {
        let config = fixtures::custom_config();
        let sdk = DownloadClient::with_config(config.clone()).unwrap();
        let image_hit = fixtures::sample_image_hit();
        let model_hit = fixtures::sample_model_hit();

        assert_eq!(sdk.config(), &config);
        assert_eq!(
            sdk.image_page_url(&image_hit),
            "https://alt.civitai.test/images/1"
        );
        assert_eq!(
            sdk.original_media_url(&image_hit).as_deref(),
            Some("https://media.civitai.test/custom-space/abc123-token/original=true")
        );
        assert_eq!(
            sdk.media_url_with_namespace(&image_hit, "other-space")
                .as_deref(),
            Some("https://media.civitai.test/other-space/abc123-token/original=true")
        );
        assert_eq!(
            sdk.model_page_url(&model_hit),
            "https://alt.civitai.test/models/12345"
        );
        assert_eq!(
            sdk.model_download_url(&model_hit).as_deref(),
            Some("https://download.civitai.test/models/987654")
        );
        assert_eq!(
            sdk.model_download_url_with_token(&model_hit, "secret")
                .as_deref(),
            Some("https://download.civitai.test/models/987654?token=secret")
        );
        assert_eq!(
            sdk.build_model_download_url(777),
            "https://download.civitai.test/models/777"
        );
        assert_eq!(
            sdk.build_model_download_url_with_token(777, "secret"),
            "https://download.civitai.test/models/777?token=secret"
        );
    }
}

mod live_tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn fetch_live_civitai_image_web_search_sample() -> Result<(), Box<dyn std::error::Error>>
    {
        let sdk = WebSearchClient::new()?;
        let state = ImageSearchState {
            query: Some("man".to_string()),
            tags: vec!["sg".to_string()],
            sort_by: ImageSearchSortBy::MostReactions,
            limit: Some(2),
            ..Default::default()
        };
        let typed = sdk.search_images(&state).await?;
        let value: Value = sdk.search_images_raw(&state).await?;

        let items = value["hits"].as_array().cloned().unwrap_or_default();
        let metadata = json!({
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
            json!({
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
                hit.original_media_url()
                    .unwrap_or_else(|| "N/A".to_string())
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
            let prompt = item.get("prompt").and_then(Value::as_str).unwrap_or("N/A");
            let page_url = format!("https://civitai.com/images/{id}");
            let media_url = item
                .get("url")
                .and_then(Value::as_str)
                .map(|token| {
                    format!(
                        "https://image.civitai.com/xG1nkqKTMzGDvpLrqFT7WA/{token}/original=true"
                    )
                })
                .unwrap_or_else(|| "N/A".to_string());
            println!(
                "item[{idx}] id={id}, username={username}, baseModel={base_model}, prompt_len={}, page_url={page_url}, media_url={media_url}",
                prompt.len()
            );
        }

        assert!(!items.is_empty());
        assert_eq!(typed.hits.len(), items.len());
        dump_items("item", &items);
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn fetch_live_civitai_image_only_web_search_sample()
    -> Result<(), Box<dyn std::error::Error>> {
        let sdk = WebSearchClient::new()?;
        let state = ImageSearchState {
            query: Some("hello".to_string()),
            media_types: vec![ImageMediaType::Image],
            limit: Some(5),
            ..Default::default()
        };
        let typed = sdk.search_images(&state).await?;

        println!(
            "image_only_response_summary = {}",
            json!({
                "hits": typed.hits.len(),
                "estimatedTotalHits": typed.estimated_total_hits,
                "processingTimeMs": typed.processing_time_ms,
                "limit": typed.limit,
                "offset": typed.offset,
            })
        );

        for (idx, hit) in typed.hits.iter().take(5).enumerate() {
            println!(
                "image_only_item[{idx}] id={}, type={}, page_url={}",
                hit.id,
                hit.r#type.as_deref().unwrap_or("N/A"),
                hit.image_page_url()
            );
        }

        assert!(!typed.hits.is_empty());
        assert!(
            typed
                .hits
                .iter()
                .all(|hit| matches!(hit.r#type.as_deref(), Some("image")))
        );
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn fetch_live_civitai_video_only_web_search_sample()
    -> Result<(), Box<dyn std::error::Error>> {
        let sdk = WebSearchClient::new()?;
        let candidate_queries = ["video", "animation", "wan"];
        let mut typed = None;

        for query in candidate_queries {
            let response = sdk
                .search_images(&ImageSearchState {
                    query: Some(query.to_string()),
                    media_types: vec![ImageMediaType::Video],
                    limit: Some(5),
                    ..Default::default()
                })
                .await?;
            if !response.hits.is_empty() {
                typed = Some(response);
                break;
            }
        }

        let typed = typed.ok_or("no video-only results found from live search")?;

        println!(
            "video_only_response_summary = {}",
            json!({
                "hits": typed.hits.len(),
                "estimatedTotalHits": typed.estimated_total_hits,
                "processingTimeMs": typed.processing_time_ms,
                "limit": typed.limit,
                "offset": typed.offset,
            })
        );

        for (idx, hit) in typed.hits.iter().take(5).enumerate() {
            println!(
                "video_only_item[{idx}] id={}, type={}, page_url={}",
                hit.id,
                hit.r#type.as_deref().unwrap_or("N/A"),
                hit.image_page_url()
            );
        }

        assert!(!typed.hits.is_empty());
        assert!(
            typed
                .hits
                .iter()
                .all(|hit| matches!(hit.r#type.as_deref(), Some("video")))
        );
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn fetch_live_civitai_model_web_search_sample() -> Result<(), Box<dyn std::error::Error>>
    {
        let sdk = WebSearchClient::new()?;
        let state = ModelSearchState {
            // query: Some("hello".to_string()),
            // tags: vec!["anime".to_string()],
            sort_by: ModelSearchSortBy::HighestRated,
            limit: Some(2),
            ..Default::default()
        };
        let typed = sdk.search_models(&state).await?;
        let value: Value = sdk.search_models_raw(&state).await?;

        let items = value["hits"].as_array().cloned().unwrap_or_default();
        let metadata = json!({
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
            json!({
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
        dump_items("model_item", &items);
        Ok(())
    }

    fn dump_items(prefix: &str, items: &[Value]) {
        for (idx, item) in items.iter().take(3).enumerate() {
            let pretty = serde_json::to_string_pretty(item).unwrap_or_else(|_| "{}".to_string());
            println!("{prefix}[{idx}] full_json = {pretty}");
            if let Some(map) = item.as_object() {
                let keys = map.keys().cloned().collect::<Vec<_>>();
                println!("{prefix}[{idx}] keys = {:?}", keys);
                for key in keys {
                    let rendered = item
                        .get(&key)
                        .map(|value| {
                            serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
                        })
                        .unwrap_or_else(|| "null".to_string());
                    println!("{prefix}[{idx}].{key} = {rendered}");
                }
            }
        }
    }
}
