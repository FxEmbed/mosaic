# FxEmbed Mosaic
![](https://skillicons.dev/icons?i=rust)

Mosaic is the name of FxEmbed's multi-image combining component. It takes a list of images in its URL and automatically pulls and stitches them.

Example URL: `https://mosaic.fxtwitter.com/jpeg/1692367302300172424/F3x-ebzWgAACauT/F3x-eb3XUAAnEEb`

Wherein the schema is /:format/:tweet_id/:list_of/:image_ids. Up to 4 images may be specified. JPEG and WebP are supported as formats. WebP takes considerably longer to compress, but provides smaller images. FixTweet currently only natively uses JPEG for the broadest compatibility and fastest response times for users.

Mosaic is written in Rust for its balance of blazing fast performance (very important here!), memory safety, and availability of 3rd party Cargo packages.

The default http port is 3030. You can override this by passing through an environment variable `PORT`.

Note: This server does not provide its own cache management solution. We assume you are running this behind a reverse proxy or CDN (i.e. Cloudflare) that caches image responses for you for when multiple requests are made to the same image.

## Building

This is not a full tutorial on how to build and use Rust, but TL;DR:

1. Make sure you have [Rust/Cargo and necessary OS build dependencies installed](https://rustup.rs/)
2. Run `cargo build --release` in the repository
3. You can now run `target/release/mosaic` to start the server

You can also build a Docker image with `docker build -t mosaic .` and run it with `docker run -p 3030:3030 mosaic`.

Credits:
- [Antonio32A](https://github.com/Antonio32A) (writing Rust version)
- [dangered wolf](https://github.com/dangeredwolf) ([Original TypeScript reference](https://github.com/FixTweet/mosaic-reference) and minor improvements)
- [Deer-Spangle](https://github.com/Deer-Spangle) (Improved image stitching algorithm)
- [Syfaro](https://github.com/Syfaro) (Rust server/implementation improvements)


