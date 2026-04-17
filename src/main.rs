#[cfg(target_os = "linux")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

/*
 * MIT License
 *
 * Copyright (c) 2022 Antonio32A (antonio32a.com) <~@antonio32a.com>
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
    Extension, Router,
};
use serde::Deserialize;
use tokio::sync::Semaphore;
use tracing::instrument;

use crate::config::AppConfig;
use crate::mosaic::mosaic;
use crate::utils::{fetch_image, image_response};

mod config;
mod mosaic;
mod utils;

#[derive(Debug, Deserialize)]
struct HandlePath {
    image_type: ImageType,
    tweet_id: String,
    image_ids: String,
}

#[derive(Copy, Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageType {
    Webp,
    Png,
    Jpeg,
}

#[instrument(skip(path, headers, config, client, in_flight))]
async fn handle(
    State(config): State<Arc<AppConfig>>,
    headers: HeaderMap,
    path: Path<HandlePath>,
    Extension(client): Extension<reqwest::Client>,
    Extension(in_flight): Extension<Arc<Semaphore>>,
) -> impl IntoResponse {
    let Ok(_permit) = in_flight.acquire_owned().await else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Server is busy; try again shortly.",
        )
            .into_response();
    };

    let host = headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");

    let Some(provider) = config.provider_for_host(host) else {
        return (
            StatusCode::BAD_REQUEST,
            format!(
                "Unknown host: '{}'. This server expects requests on one of the \
                 configured mosaic domains (see MOSAIC_DOMAINS and BLUESKY_MOSAIC_DOMAINS).",
                host
            ),
        )
            .into_response();
    };

    let image_ids: Vec<_> = path
        .image_ids
        .split('/')
        .filter(|image_id| !image_id.is_empty())
        .collect();

    tracing::info!(
        image_type = ?path.image_type,
        tweet_id = %path.tweet_id,
        ?provider,
        "given image ids: {}",
        image_ids.join(", ")
    );

    let start = Instant::now();
    let images: Vec<_> = futures::future::join_all(
        image_ids
            .iter()
            .map(|image_id| fetch_image(&client, provider, image_id)),
    )
    .await
    .into_iter()
    .flatten()
    .collect();
    let download_time = start.elapsed();

    if images.is_empty() {
        tracing::warn!("no images were found");
        return (StatusCode::BAD_REQUEST, "No images could be found.").into_response();
    }

    let span = tracing::Span::current();

    let mosaic_start = Instant::now();
    let image = match tokio::task::spawn_blocking(move || span.in_scope(|| mosaic(images))).await {
        Ok(image) => image,
        Err(err) => {
            tracing::error!("could not spawn mosaic task: {}", err);

            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Mosaic task failed to complete.",
            )
                .into_response();
        }
    };
    let mosaic_time = mosaic_start.elapsed();
    let size = format!("{0}x{1}", image.width(), image.height());

    let encoding_start = Instant::now();
    let encoded = match image_response(image, path.image_type) {
        Ok(res) => res.into_response(),
        Err(err) => {
            tracing::error!("could not encode image: {}", err);

            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Image could not be encoded.",
            )
                .into_response();
        }
    };

    tracing::info!(
        time = start.elapsed().as_millis(),
        download = download_time.as_millis(),
        mosaic = mosaic_time.as_millis(),
        encoding = encoding_start.elapsed().as_millis(),
        "completed encode with final dimensions: {}",
        size
    );

    encoded
}

fn main() {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "info");
    }

    tracing_subscriber::fmt::init();

    let max_concurrent = std::env::var("MOSAIC_MAX_CONCURRENT")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(8_usize);

    // Each mosaic uses `spawn_blocking`; Tokio's default blocking pool can grow to hundreds
    // of threads (each with a multi-MiB stack), which shows up as RSS that never drops on glibc.
    // Keep the cap tied to how much blocking work we intentionally allow.
    let max_blocking_threads = max_concurrent
        .saturating_add(16)
        .clamp(8, 128);

    let worker_threads = std::env::var("TOKIO_WORKER_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n| n > 0);

    let mut runtime = tokio::runtime::Builder::new_multi_thread();
    runtime.enable_all().max_blocking_threads(max_blocking_threads);
    if let Some(n) = worker_threads {
        runtime.worker_threads(n);
    }

    let runtime = runtime
        .build()
        .expect("failed to build tokio runtime");

    tracing::info!(
        max_blocking_threads,
        "tokio blocking thread cap (raise MOSAIC_MAX_CONCURRENT carefully); set TOKIO_WORKER_THREADS to cap async worker threads"
    );

    let config = Arc::new(AppConfig::from_env());
    tracing::info!(
        "Twitter mosaic domains: {:?}",
        config.twitter_mosaic_domains
    );
    tracing::info!(
        "Bluesky mosaic domains: {:?}",
        config.bluesky_mosaic_domains
    );

    runtime.block_on(async_main(max_concurrent, config));
}

async fn async_main(max_concurrent: usize, config: Arc<AppConfig>) {
    let client = reqwest::ClientBuilder::new()
        .timeout(Duration::from_secs(5))
        .pool_max_idle_per_host(8)
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .unwrap();

    let in_flight = Arc::new(Semaphore::new(max_concurrent));
    tracing::info!(
        "mosaic concurrency limit: {} (set MOSAIC_MAX_CONCURRENT to override)",
        max_concurrent
    );

    let app = Router::new()
        .route(
            "/{image_type}/{tweet_id}/{*image_ids}",
            get(handle),
        )
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(Extension(client))
        .layer(Extension(in_flight))
        .with_state(config);

    let port = std::env::var("PORT")
        .unwrap_or_else(|_err| "3030".to_string())
        .parse()
        .expect("PORT was invalid");
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    tracing::info!("starting fixtweet-mosaic on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
