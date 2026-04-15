use std::env;

/// Upstream image CDN for mosaic tiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentProvider {
    /// Twitter / X — tiles are loaded from `pbs.twimg.com`.
    Twitter,
    /// Bluesky — tiles are loaded from `cdn.bsky.app`.
    Bluesky,
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    /// Hostnames (no port) that route to the Twitter mosaic pipeline.
    pub twitter_mosaic_domains: Vec<String>,
    /// Hostnames (no port) that route to the Bluesky mosaic pipeline.
    pub bluesky_mosaic_domains: Vec<String>,
}

impl AppConfig {
    /// | Variable                    | Default | Notes |
    /// |-----------------------------|-------------------------|-------|
    /// | `MOSAIC_DOMAINS`            | `mosaic.fxtwitter.com,127.0.0.1,localhost` | Comma-separated |
    /// | `BLUESKY_MOSAIC_DOMAINS`    | `mosaic.fxbsky.app`     | Comma-separated |
    pub fn from_env() -> Self {
        let twitter_mosaic_domains = parse_domain_list(
            &env::var("MOSAIC_DOMAINS").unwrap_or_else(|_| {
                "mosaic.fxtwitter.com,127.0.0.1,localhost".into()
            }),
        );
        let bluesky_mosaic_domains = parse_domain_list(
            &env::var("BLUESKY_MOSAIC_DOMAINS").unwrap_or_else(|_| "mosaic.fxbsky.app".into()),
        );
        Self {
            twitter_mosaic_domains,
            bluesky_mosaic_domains,
        }
    }

    /// Resolve [`ContentProvider`] from the HTTP `Host` header value.
    pub fn provider_for_host(&self, host: &str) -> Option<ContentProvider> {
        let hostname = host.split(':').next().unwrap_or(host).to_lowercase();

        if self.twitter_mosaic_domains.contains(&hostname) {
            Some(ContentProvider::Twitter)
        } else if self.bluesky_mosaic_domains.contains(&hostname) {
            Some(ContentProvider::Bluesky)
        } else {
            None
        }
    }
}

fn parse_domain_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}
