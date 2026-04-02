use civitai_cli::sdk::{
    DownloadClient, DownloadEvent, DownloadKind, DownloadOptions, DownloadSpec, ImageSearchState,
    ModelDownloadAuth, ModelSearchState, SearchImageHit, SearchModelHit, WebSearchClient,
};
use civitai_cli::sdk::{DownloadDestination, SearchSdkConfig};
use futures_util::StreamExt;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

fn temp_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("civitai-sdk-test-{nanos}-{name}"))
}

async fn spawn_server() -> std::io::Result<String> {
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
                let mut lines = req.lines();
                let request_line = lines.next().unwrap_or_default();
                let path = request_line.split_whitespace().nth(1).unwrap_or("/");
                let has_bearer = req_lower.contains("authorization: bearer secret-token");
                let range_start = req_lower
                    .lines()
                    .find(|line| line.starts_with("range: bytes="))
                    .and_then(|line| line.strip_prefix("range: bytes="))
                    .and_then(|line| line.strip_suffix('-'))
                    .and_then(|value| value.parse::<usize>().ok());

                match path {
                    "/file.bin" => {
                        let body = b"hello sdk download";
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\n\r\n",
                            body.len()
                        );
                        let _ = socket.write_all(response.as_bytes()).await;
                        let _ = socket.write_all(body).await;
                    }
                    "/attachment" => {
                        let body = b"attachment body";
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=\"server.bin\"\r\n\r\n",
                            body.len()
                        );
                        let _ = socket.write_all(response.as_bytes()).await;
                        let _ = socket.write_all(body).await;
                    }
                    "/resume.bin" => {
                        let body = b"0123456789";
                        let start = range_start.unwrap_or(0).min(body.len());
                        let partial = &body[start..];
                        if range_start.is_some() {
                            let response = format!(
                                "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nContent-Type: application/octet-stream\r\n\r\n",
                                partial.len(),
                                start,
                                body.len().saturating_sub(1),
                                body.len()
                            );
                            let _ = socket.write_all(response.as_bytes()).await;
                        } else {
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\n\r\n",
                                body.len()
                            );
                            let _ = socket.write_all(response.as_bytes()).await;
                        }
                        let _ = socket.write_all(partial).await;
                    }
                    "/auth-query?token=secret-token" => {
                        let body = b"auth query ok";
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\n\r\n",
                            body.len()
                        );
                        let _ = socket.write_all(response.as_bytes()).await;
                        let _ = socket.write_all(body).await;
                    }
                    "/auth-bearer" if has_bearer => {
                        let body = b"auth bearer ok";
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\n\r\n",
                            body.len()
                        );
                        let _ = socket.write_all(response.as_bytes()).await;
                        let _ = socket.write_all(body).await;
                    }
                    "/image-noext" => {
                        let body = b"\x89PNG\r\n\x1a\npng";
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: image/png\r\n\r\n",
                            body.len()
                        );
                        let _ = socket.write_all(response.as_bytes()).await;
                        let _ = socket.write_all(body).await;
                    }
                    _ => {
                        let body = b"not found";
                        let response = format!(
                            "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nContent-Type: text/plain\r\n\r\n",
                            body.len()
                        );
                        let _ = socket.write_all(response.as_bytes()).await;
                        let _ = socket.write_all(body).await;
                    }
                }
            });
        }
    });

    Ok(format!("http://{}", addr))
}

fn sample_image_hit(kind: &str) -> SearchImageHit {
    SearchImageHit {
        id: 77,
        url: Some("media-token".to_string()),
        width: None,
        height: None,
        r#type: Some(kind.to_string()),
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
    }
}

fn sample_model_hit() -> SearchModelHit {
    SearchModelHit {
        id: 55,
        name: Some("My / Fancy Model".to_string()),
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
        version: Some(serde_json::json!({ "id": 999 })),
        versions: None,
        images: None,
        can_generate: None,
        nsfw: None,
        nsfw_level: None,
        extras: serde_json::json!({}),
    }
}

#[test]
fn builds_download_specs_from_hits() {
    let client = DownloadClient::with_config(
        SearchSdkConfig::builder()
            .civitai_web_url("https://civitai.test")
            .media_delivery_url("https://media.test")
            .media_delivery_namespace("ns")
            .model_download_api_url("https://download.test/models")
            .build_config(),
    )
    .unwrap();

    let image_hit = sample_image_hit("image");
    let video_hit = sample_image_hit("video");
    let model_hit = sample_model_hit();

    let image_spec = client.build_media_download_spec(&image_hit).unwrap();
    assert_eq!(image_spec.kind, DownloadKind::Image);
    assert_eq!(image_spec.file_name.as_deref(), Some("civitai-image-77.png"));
    assert_eq!(
        image_spec.url,
        "https://media.test/ns/media-token/original=true"
    );

    let video_spec = client.build_video_download_spec(&video_hit).unwrap();
    assert_eq!(video_spec.kind, DownloadKind::Video);
    assert_eq!(video_spec.file_name.as_deref(), Some("civitai-video-77.mp4"));

    let model_spec = client
        .build_model_download_spec(
            &model_hit,
            Some(ModelDownloadAuth::QueryToken("secret-token".to_string())),
        )
        .unwrap();
    assert_eq!(model_spec.kind, DownloadKind::Model);
    assert_eq!(
        model_spec.file_name.as_deref(),
        Some("My _ Fancy Model-v999")
    );
    assert_eq!(
        model_spec.url,
        "https://download.test/models/999?token=secret-token"
    );
}

#[tokio::test]
async fn downloads_to_explicit_file_path() -> Result<(), Box<dyn std::error::Error>> {
    let base_url = spawn_server().await?;
    let client = DownloadClient::new()?;
    let target = temp_path("explicit.bin");

    let spec = DownloadSpec::new(format!("{base_url}/file.bin"), DownloadKind::Other)
        .with_file_name("ignored.bin");
    let result = client
        .download(&spec, &DownloadOptions::to_file(&target), None, None)
        .await?;

    assert_eq!(result.path, target);
    assert_eq!(tokio::fs::read(&result.path).await?, b"hello sdk download");
    assert_eq!(result.total_bytes, Some(18));
    Ok(())
}

#[tokio::test]
async fn downloads_to_directory_and_uses_server_filename() -> Result<(), Box<dyn std::error::Error>>
{
    let base_url = spawn_server().await?;
    let client = DownloadClient::new()?;
    let target_dir = temp_path("downloads-dir");

    let spec = DownloadSpec::new(format!("{base_url}/attachment"), DownloadKind::Other);
    let result = client
        .download(
            &spec,
            &DownloadOptions {
                destination: DownloadDestination::Directory(target_dir.clone()),
                ..DownloadOptions::default()
            },
            None,
            None,
        )
        .await?;

    assert_eq!(result.path, target_dir.join("server.bin"));
    assert_eq!(tokio::fs::read(&result.path).await?, b"attachment body");
    Ok(())
}

#[tokio::test]
async fn resumes_partial_download_and_emits_events() -> Result<(), Box<dyn std::error::Error>> {
    let base_url = spawn_server().await?;
    let client = DownloadClient::new()?;
    let target = temp_path("resume.bin");
    tokio::fs::write(&target, b"01234").await?;

    let (tx, mut rx) = mpsc::channel(32);
    let spec = DownloadSpec::new(format!("{base_url}/resume.bin"), DownloadKind::Other);
    let result = client
        .download(&spec, &DownloadOptions::to_file(&target), Some(tx), None)
        .await?;

    assert_eq!(tokio::fs::read(&result.path).await?, b"0123456789");
    assert!(result.resumed);

    let mut saw_started = false;
    let mut saw_completed = false;
    while let Ok(event) = rx.try_recv() {
        match event {
            DownloadEvent::Started { resumed, .. } => {
                assert!(resumed);
                saw_started = true;
            }
            DownloadEvent::Completed { .. } => {
                saw_completed = true;
            }
            _ => {}
        }
    }
    assert!(saw_started);
    assert!(saw_completed);
    Ok(())
}

#[tokio::test]
async fn downloads_with_query_and_bearer_auth() -> Result<(), Box<dyn std::error::Error>> {
    let base_url = spawn_server().await?;
    let client = DownloadClient::new()?;

    let query_target = temp_path("query-auth.bin");
    let query_spec = DownloadSpec::new(format!("{base_url}/auth-query"), DownloadKind::Model)
        .with_auth(ModelDownloadAuth::QueryToken("secret-token".to_string()));
    client
        .download(
            &query_spec,
            &DownloadOptions::to_file(&query_target),
            None,
            None,
        )
        .await?;
    assert_eq!(tokio::fs::read(&query_target).await?, b"auth query ok");

    let bearer_target = temp_path("bearer-auth.bin");
    let bearer_spec = DownloadSpec::new(format!("{base_url}/auth-bearer"), DownloadKind::Model)
        .with_auth(ModelDownloadAuth::BearerToken("secret-token".to_string()));
    client
        .download(
            &bearer_spec,
            &DownloadOptions::to_file(&bearer_target),
            None,
            None,
        )
        .await?;
    assert_eq!(tokio::fs::read(&bearer_target).await?, b"auth bearer ok");
    Ok(())
}

#[tokio::test]
async fn infers_extension_for_file_destination_without_one() -> Result<(), Box<dyn std::error::Error>>
{
    let base_url = spawn_server().await?;
    let client = DownloadClient::new()?;
    let target = temp_path("image-download");

    let spec = DownloadSpec::new(format!("{base_url}/image-noext"), DownloadKind::Image)
        .with_file_name("civitai-image-77");
    let result = client
        .download(&spec, &DownloadOptions::to_file(&target), None, None)
        .await?;

    assert_eq!(result.path.extension().and_then(|value| value.to_str()), Some("png"));
    assert_eq!(tokio::fs::read(&result.path).await?, b"\x89PNG\r\n\x1a\npng");
    Ok(())
}

async fn download_first_chunk_to_temp(
    client: &DownloadClient,
    spec: &DownloadSpec,
    name: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let response = client
        .build_download_request(&spec.url, spec.auth.as_ref(), Some(0))?
        .send()
        .await?
        .error_for_status()?;

    let mut stream = response.bytes_stream();
    let first_chunk = stream.next().await.ok_or("empty download response")??;

    let target = temp_path(name);
    tokio::fs::write(&target, &first_chunk).await?;
    assert!(!first_chunk.is_empty());
    Ok(target)
}

#[tokio::test]
#[ignore]
async fn fetch_live_civitai_image_download_sample() -> Result<(), Box<dyn std::error::Error>> {
    let web = WebSearchClient::new()?;
    let download = DownloadClient::new()?;
    let response = web
        .search_images(&ImageSearchState {
            query: Some("landscape".to_string()),
            tags: vec!["pg".to_string()],
            limit: Some(10),
            ..Default::default()
        })
        .await?;

    let hit = response
        .hits
        .iter()
        .find(|hit| !matches!(hit.r#type.as_deref(), Some("video")))
        .ok_or("no image hit found")?;
    let spec = download
        .build_image_download_spec(hit)
        .ok_or("failed to build image download spec")?;
    let path = download_first_chunk_to_temp(&download, &spec, "live-image.bin").await?;
    let size = tokio::fs::metadata(&path).await?.len();

    println!("live_image_download_url={}", spec.url);
    println!("live_image_download_path={}", path.display());
    println!("live_image_download_bytes={size}");

    assert!(size > 0);
    Ok(())
}

#[tokio::test]
#[ignore]
async fn fetch_live_civitai_video_download_sample() -> Result<(), Box<dyn std::error::Error>> {
    let web = WebSearchClient::new()?;
    let download = DownloadClient::new()?;
    let candidate_queries = ["video", "animation", "wan"];
    let mut video_hit = None;

    for query in candidate_queries {
        let response = web
            .search_images(&ImageSearchState {
                query: Some(query.to_string()),
                limit: Some(20),
                ..Default::default()
            })
            .await?;
        if let Some(hit) = response
            .hits
            .into_iter()
            .find(|hit| matches!(hit.r#type.as_deref(), Some("video")))
        {
            video_hit = Some(hit);
            break;
        }
    }

    let hit = video_hit.ok_or("no video hit found from live search")?;
    let spec = download
        .build_video_download_spec(&hit)
        .ok_or("failed to build video download spec")?;
    let path = download_first_chunk_to_temp(&download, &spec, "live-video.bin").await?;
    let size = tokio::fs::metadata(&path).await?.len();

    println!("live_video_download_url={}", spec.url);
    println!("live_video_download_path={}", path.display());
    println!("live_video_download_bytes={size}");

    assert!(size > 0);
    Ok(())
}

#[tokio::test]
#[ignore]
async fn fetch_live_civitai_model_download_sample() -> Result<(), Box<dyn std::error::Error>> {
    let web = WebSearchClient::new()?;
    let download = DownloadClient::new()?;
    let response = web
        .search_models(&ModelSearchState {
            query: Some("hello".to_string()),
            tags: vec!["anime".to_string()],
            limit: Some(10),
            ..Default::default()
        })
        .await?;

    let hit = response.hits.first().ok_or("no model hit found")?;
    let spec = download
        .build_model_download_spec(hit, None)
        .ok_or("failed to build model download spec")?;
    let path = download_first_chunk_to_temp(&download, &spec, "live-model.bin").await?;
    let size = tokio::fs::metadata(&path).await?.len();

    println!("live_model_download_url={}", spec.url);
    println!("live_model_download_path={}", path.display());
    println!("live_model_download_bytes={size}");

    assert!(size > 0);
    Ok(())
}
