use civitai_cli::sdk::{
    DownloadEvent, DownloadKind, DownloadOptions, DownloadSpec,
    ModelDownloadAuth, SearchImageHit, SearchModelHit, SearchSdkClient,
};
use civitai_cli::sdk::{DownloadDestination, SearchSdkConfig};
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
                let path = request_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/");
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
    }
}

#[test]
fn builds_download_specs_from_hits() {
    let client = SearchSdkClient::with_config(
        SearchSdkConfig::builder()
            .civitai_web_url("https://civitai.test")
            .media_delivery_url("https://media.test")
            .media_delivery_namespace("ns")
            .model_download_api_url("https://download.test/models")
            .build(),
    )
    .unwrap();

    let image_hit = sample_image_hit("image");
    let video_hit = sample_image_hit("video");
    let model_hit = sample_model_hit();

    let image_spec = client.build_media_download_spec(&image_hit).unwrap();
    assert_eq!(image_spec.kind, DownloadKind::Image);
    assert_eq!(image_spec.file_name.as_deref(), Some("civitai-image-77"));
    assert_eq!(
        image_spec.url,
        "https://media.test/ns/media-token/original=true"
    );

    let video_spec = client.build_video_download_spec(&video_hit).unwrap();
    assert_eq!(video_spec.kind, DownloadKind::Video);
    assert_eq!(video_spec.file_name.as_deref(), Some("civitai-video-77"));

    let model_spec = client
        .build_model_download_spec(
            &model_hit,
            Some(ModelDownloadAuth::QueryToken("secret-token".to_string())),
        )
        .unwrap();
    assert_eq!(model_spec.kind, DownloadKind::Model);
    assert_eq!(model_spec.file_name.as_deref(), Some("My _ Fancy Model-v999"));
    assert_eq!(model_spec.url, "https://download.test/models/999?token=secret-token");
}

#[tokio::test]
async fn downloads_to_explicit_file_path() -> Result<(), Box<dyn std::error::Error>> {
    let base_url = spawn_server().await?;
    let client = SearchSdkClient::new()?;
    let target = temp_path("explicit.bin");

    let spec = DownloadSpec::new(format!("{base_url}/file.bin"), DownloadKind::Other)
        .with_file_name("ignored.bin");
    let result = client
        .download(
            &spec,
            &DownloadOptions::to_file(&target),
            None,
            None,
        )
        .await?;

    assert_eq!(result.path, target);
    assert_eq!(tokio::fs::read(&result.path).await?, b"hello sdk download");
    assert_eq!(result.total_bytes, Some(18));
    Ok(())
}

#[tokio::test]
async fn downloads_to_directory_and_uses_server_filename() -> Result<(), Box<dyn std::error::Error>> {
    let base_url = spawn_server().await?;
    let client = SearchSdkClient::new()?;
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
    let client = SearchSdkClient::new()?;
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
    let client = SearchSdkClient::new()?;

    let query_target = temp_path("query-auth.bin");
    let query_spec = DownloadSpec::new(
        format!("{base_url}/auth-query"),
        DownloadKind::Model,
    )
    .with_auth(ModelDownloadAuth::QueryToken("secret-token".to_string()));
    client
        .download(&query_spec, &DownloadOptions::to_file(&query_target), None, None)
        .await?;
    assert_eq!(tokio::fs::read(&query_target).await?, b"auth query ok");

    let bearer_target = temp_path("bearer-auth.bin");
    let bearer_spec = DownloadSpec::new(
        format!("{base_url}/auth-bearer"),
        DownloadKind::Model,
    )
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
