//! Typed Topcoat asset URLs backed by the exact files embedded in this binary.
//! The manifest is built from those bytes so a separate deployment bundle cannot
//! drift from the executable. Legacy fixed URLs remain available for clients.
use super::*;
use sha2::{Digest, Sha256};
use topcoat::asset::{Asset, AssetConfig, Manifest, ManifestEntry, asset};

pub(super) const STYLESHEET: Asset = asset!("static/css/site.css");
pub(super) const THEME: Asset = asset!("static/js/theme.js");
pub(super) const CLIENT: Asset = asset!("static/js/app.js");
pub(super) const ICON: Asset = asset!("static/icon.svg");
pub(super) const WEB_MANIFEST: Asset = asset!("static/manifest.webmanifest");
path_param!(asset_file);

struct EmbeddedAsset {
    legacy: &'static str,
    file: String,
    source: &'static str,
    content_type: &'static str,
}
pub(super) struct EmbeddedAssets(Vec<EmbeddedAsset>);

impl EmbeddedAssets {
    pub(super) fn load() -> (Self, AssetConfig) {
        let mut assets = Vec::new();
        let mut manifest = Manifest {
            version: topcoat::asset::MANIFEST_VERSION,
            assets: Vec::new(),
        };
        for (handle, legacy, source, content_type) in [
            (
                STYLESHEET,
                "site.css",
                include_str!("../../static/css/site.css"),
                "text/css; charset=utf-8",
            ),
            (
                THEME,
                "theme.js",
                include_str!("../../static/js/theme.js"),
                "text/javascript; charset=utf-8",
            ),
            (
                CLIENT,
                "app.js",
                include_str!("../../static/js/app.js"),
                "text/javascript; charset=utf-8",
            ),
            (
                ICON,
                "icon.svg",
                include_str!("../../static/icon.svg"),
                "image/svg+xml",
            ),
            (
                WEB_MANIFEST,
                "manifest.webmanifest",
                include_str!("../../static/manifest.webmanifest"),
                "application/manifest+json",
            ),
        ] {
            let hash = hex::encode(Sha256::digest(source.as_bytes()));
            let (stem, extension) = legacy.rsplit_once('.').expect("asset extension");
            let file = format!("{stem}-{}.{extension}", &hash[..16]);
            manifest.assets.push(ManifestEntry {
                id: handle.id(),
                file: file.clone(),
                hash,
                content_type: content_type.into(),
            });
            assets.push(EmbeddedAsset {
                legacy,
                file,
                source,
                content_type,
            });
        }
        (
            Self(assets),
            AssetConfig::hosted_at("/_topcoat/assets", manifest),
        )
    }
}

fn respond(cx: &Cx, legacy: bool) -> topcoat::Result<Response> {
    let name = path_param::<AssetFile>(cx);
    let asset = app_context::<EmbeddedAssets>(cx)
        .0
        .iter()
        .find(|asset| {
            if legacy {
                asset.legacy == name
            } else {
                asset.file == name
            }
        })
        .ok_or_else(not_found)?;
    let mut response = Response::new(Body::from(asset.source));
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static(asset.content_type),
    );
    response.headers_mut().insert(
        http::header::CACHE_CONTROL,
        http::HeaderValue::from_static(if legacy {
            "no-cache"
        } else {
            "public, max-age=31536000, immutable"
        }),
    );
    Ok(response)
}

#[route(GET "/_topcoat/assets/{asset_file}")]
async fn bundled_asset(cx: &Cx) -> topcoat::Result<Response> {
    respond(cx, false)
}

#[route(GET "/assets/{asset_file}")]
async fn legacy_asset(cx: &Cx) -> topcoat::Result<Response> {
    respond(cx, true)
}
