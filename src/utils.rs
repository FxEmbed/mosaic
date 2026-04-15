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

use std::time::Instant;

use axum::{
    http::{header, StatusCode},
    response::IntoResponse,
};
use bytes::BytesMut;
use const_format::formatcp;
use image::{
    codecs::{jpeg::JpegEncoder, png::PngEncoder},
    EncodableLayout, ImageEncoder, ImageError, RgbImage,
};
use lazy_static::lazy_static;
use reqwest::header::{HeaderMap, HeaderValue};
use tracing::instrument;

use crate::config::ContentProvider;
use crate::ImageType;

const FAKE_CHROME_VERSION: &str = "103";
const MAX_IMAGE_SIZE: usize = 10_000_000;

lazy_static! {
    static ref FETCH_HEADERS: HeaderMap = {
        let mut headers = HeaderMap::new();

        headers.append("sec-ch-ua", HeaderValue::from_static(formatcp!("\".Not/A)Brand\";v=\"99\", \"Google Chrome\";v=\"{version}\", \"Chromium\";v=\"{version}\"", version = FAKE_CHROME_VERSION)));
        headers.append("DNT", HeaderValue::from_static("1"));
        headers.append("x-twitter-client-language", HeaderValue::from_static("en"));
        headers.append("sec-ch-ua-mobile", HeaderValue::from_static("?0"));
        headers.append(
            "content-type",
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        headers.append("User-Agent", HeaderValue::from_static(formatcp!("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{version}.0.0.0 Safari/537.36", version = FAKE_CHROME_VERSION)));
        headers.append("x-twitter-active-user", HeaderValue::from_static("yes"));
        headers.append(
            "sec-ch-ua-platform",
            HeaderValue::from_static("\"Windows\""),
        );
        headers.append("Accept", HeaderValue::from_static("*/*"));
        headers.append("Origin", HeaderValue::from_static("https://twitter.com"));
        headers.append("Sec-Fetch-Site", HeaderValue::from_static("same-site"));
        headers.append("Sec-Fetch-Mode", HeaderValue::from_static("cors"));
        headers.append("Sec-Fetch-Dest", HeaderValue::from_static("empty"));
        headers.append("Referer", HeaderValue::from_static("https://twitter.com/"));
        headers.append(
            "Accept-Encoding",
            HeaderValue::from_static("gzip, deflate, br"),
        );
        headers.append("Accept-Language", HeaderValue::from_static("en"));

        headers
    };
}

pub fn image_response(img: RgbImage, encoder: ImageType) -> Result<impl IntoResponse, ImageError> {
    let encoded = match encoder {
        ImageType::Webp => webp::Encoder::from_rgb(img.as_bytes(), img.width(), img.height())
            .encode(90.0)
            .to_vec(),

        ImageType::Png => {
            let mut out = vec![];
            let enc = PngEncoder::new(&mut out);
            enc.write_image(
                img.as_bytes(),
                img.width(),
                img.height(),
                image::ColorType::Rgb8.into(),
            )?;
            out.to_vec()
        }

        ImageType::Jpeg => {
            let mut out = vec![];
            let enc = JpegEncoder::new(&mut out);
            enc.write_image(
                img.as_bytes(),
                img.width(),
                img.height(),
                image::ColorType::Rgb8.into(),
            )?;
            out.to_vec()
        }
    };

    let content_type = match encoder {
        ImageType::Webp => "image/webp",
        ImageType::Png => "image/png",
        ImageType::Jpeg => "image/jpeg",
    };

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, content_type)],
        encoded,
    ))
}

#[instrument(skip(client))]
pub async fn fetch_image(
    client: &reqwest::Client,
    provider: ContentProvider,
    id: &str,
) -> Option<RgbImage> {
    match provider {
        ContentProvider::Twitter => fetch_twitter_image(client, id).await,
        ContentProvider::Bluesky => fetch_bluesky_image(client, id).await,
    }
}

async fn fetch_twitter_image(client: &reqwest::Client, id: &str) -> Option<RgbImage> {
    tracing::trace!("starting to download image");

    let start = Instant::now();

    let mut resp = client
        .get(format!(
            "https://pbs.twimg.com/media/{}?format=jpg&name=large",
            id
        ))
        .headers(FETCH_HEADERS.clone())
        .send()
        .await
        .ok()?;

    download_image_body(&mut resp, start).await
}

async fn fetch_bluesky_image(client: &reqwest::Client, id: &str) -> Option<RgbImage> {
    tracing::trace!("starting to download Bluesky image");

    let start = Instant::now();
    let path_id = id.replace('_', "/");

    let mut resp = client
        .get(format!(
            "https://cdn.bsky.app/img/feed_fullsize/plain/{}@jpeg",
            path_id
        ))
        .headers(FETCH_HEADERS.clone())
        .send()
        .await
        .ok()?;

    download_image_body(&mut resp, start).await
}

async fn download_image_body(resp: &mut reqwest::Response, start: Instant) -> Option<RgbImage> {
    let mut buf = BytesMut::new();

    while let Some(chunk) = resp.chunk().await.ok()? {
        if buf.len() + chunk.len() > MAX_IMAGE_SIZE {
            tracing::warn!("image was too large, skipping");
            return None;
        }

        buf.extend(chunk);
    }

    tracing::debug!(
        bytes = buf.len(),
        time = start.elapsed().as_millis(),
        "downloaded image"
    );

    match image::load_from_memory(&buf) {
        Ok(im) => Some(im.into_rgb8()),
        Err(err) => {
            tracing::warn!("image could not be loaded: {}", err);
            None
        }
    }
}
