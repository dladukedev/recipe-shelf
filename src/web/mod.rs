use rand::{RngCore, rngs::OsRng};
use serde::Deserialize;
use topcoat::{
    context::{Cx, app_context, memoize},
    cookie::{Cookie, Cookies, RouterBuilderCookieExt, SameSite, cookies},
    router::{
        Body, LayerFn, Next, Router, RouterBuilderDiscoverExt,
        content::{Form, Json},
        error::{
            bad_request, forbidden, internal_server_error, not_found, redirect, see_other,
            too_many_requests, unauthorized,
        },
        href, page, path_param,
        response::{IntoResponse, Response},
        route,
    },
    view::{Child, View, view},
};

use crate::core::{App, Recipe, User};
use crate::exports::{ExportError, ExportFile};

const SESSION: &str = "recipe_session";
const CSRF: &str = "recipe_csrf";
const SESSION_AGE: i64 = crate::core::TOKEN_TTL;
path_param!(recipe_id: i64, error = bad_request);
path_param!(api_recipe_id: i64, error = bad_request);
path_param!(export_format);

use crate::exports::format_minutes;

#[cfg(test)]
mod time_tests {
    #[test]
    fn duration_labels_handle_hour_boundaries_and_zero() {
        for (minutes, expected) in [
            (0, "0 min"),
            (59, "59 min"),
            (60, "1 hr"),
            (61, "1 hr 1 min"),
            (120, "2 hrs"),
            (145, "2 hrs 25 min"),
        ] {
            assert_eq!(super::format_minutes(minutes), expected);
        }
    }
}

mod context;
mod errors;
use errors::*;
mod api;
mod assets;
mod auth;
mod collection;
mod components;
mod downloads;
mod login;
mod preview;
mod query;
mod recipes;
mod security;
mod settings;
mod throttle;

use auth::*;
use components::*;
use context::*;
use query::*;
use security::security_headers;
use throttle::{LoginThrottle, login_attempt};

#[route(GET "/health")]
async fn health() -> topcoat::Result<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({"status": "ok"})))
}

pub fn router(app: App) -> Router {
    use topcoat::asset::RouterBuilderAssetExt;
    let (embedded_assets, asset_config) = assets::EmbeddedAssets::load();
    Router::builder()
        .discover()
        .cookies()
        .assets(asset_config)
        .app_context(embedded_assets)
        .app_context(app)
        .app_context(LoginThrottle::default())
        .layer(LayerFn::new(None::<&str>, |cx, body, next| {
            Box::pin(security_headers(cx, body, next))
        }))
        .build()
}
