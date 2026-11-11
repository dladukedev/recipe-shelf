use super::*;
use topcoat::view::component;

pub(super) fn shell<'a>(
    cx: &'a Cx,
    title: String,
    content: Child<'a>,
    logged: bool,
) -> impl View + 'a {
    view! { cx => document(title: title, logged: logged, child: content) }
}

#[component]
async fn document(
    cx: &Cx,
    title: String,
    logged: bool,
    child: Child<'_>,
) -> topcoat::Result<impl View> {
    let path = topcoat::router::request::uri(cx).path();
    let page_info = if path == "/" || path == "/login" || path.starts_with("/recipes/") {
        None
    } else if path.starts_with("/settings") {
        Some("Settings".to_owned())
    } else if path.starts_with("/exports") {
        Some("Export".to_owned())
    } else {
        Some(title.clone())
    };
    let home_url = if path.starts_with("/recipes/") {
        list_return(cx)?
    } else {
        href!(crate::web::recipes::home).resolve(cx)
    };
    Ok(view! {
        cx =>
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <meta name="color-scheme" content="light dark" />
                <link rel="icon" type="image/svg+xml" sizes="any" href=(assets::ICON) />
                <link rel="manifest" href=(assets::WEB_MANIFEST) />
                <title>
                    (title)
                    " · Recipe Shelf"
                </title>
                <script src=(assets::THEME)></script>
                <link rel="stylesheet" href=(assets::STYLESHEET) />
                <script defer=(true) src=(assets::CLIENT)></script>
            </head>
            <body>
                <a class="skip-link" href="#main">"Skip to content"</a>
                <header class="site-header">
                    <div class="shell header-inner">
                        <div class="header-identity">
                            <a
                                class="brand"
                                href=(home_url)
                                aria-label="Recipe Shelf — all recipes"
                            >
                                <img class="brand-mark" src=(assets::ICON) width="39" height="39" alt="" />
                                <span>"Recipe Shelf"</span>
                            </a>
                            if let Some(info) = page_info {
                                <span class="page-context">
                                    " - "
                                    (info)
                                </span>
                            }
                        </div>
                        if logged {
                            account_menu()
                        } else {
                            <a class="button button-secondary" href=(href!(crate::web::login::login_get))>
                                "Login"
                            </a>
                        }
                    </div>
                </header>
                <main class="shell main-content" id="main" tabindex="-1">(child)</main>
            </body>
        </html>
    })
}

#[component]
async fn account_menu(cx: &Cx) -> topcoat::Result<impl View> {
    Ok(view! {
        cx =>
        <details class="account-menu" data-menu=(true)>
            <summary
                class="icon-button"
                aria-label="Account options"
                title="Account options"
            >
                <svg
                    viewBox="0 0 24 24"
                    width="18"
                    height="18"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.5"
                    aria-hidden="true"
                >
                    <circle cx="12" cy="8" r="3.5" />
                    <path d="M5 21v-2a7 7 0 0 1 14 0v2" />
                </svg>
            </summary>
            <nav class="menu-options" aria-label="Account">
                <a href=(href!(crate::web::settings::basic::settings))>"Settings"</a>
                <a href=(href!(crate::web::collection::collection_export))>
                    "Export all"
                </a>
                <form method="post" action=(href!(crate::web::login::logout))>
                    csrf_field()
                    <button type="submit">"Logout"</button>
                </form>
            </nav>
        </details>
    })
}

#[component]
pub(super) async fn csrf_field(cx: &Cx) -> topcoat::Result<impl View> {
    Ok(view! { cx => <input type="hidden" name="csrf" value=(csrf_token(cx)) /> })
}

#[component]
pub(super) async fn recipe_row(cx: &Cx, recipe: &Recipe, from: &str) -> topcoat::Result<impl View> {
    Ok(view! {
        cx =>
        <a
            class="recipe-row"
            id=(format!("recipe-{}", recipe.id))
            data-recipe-link=(true)
            href=(recipe_url(cx, recipe.id, from))
        >
            <h2>(recipe.data.title.as_str())</h2>
            <p class="recipe-summary">(recipe.data.description.as_str())</p>
        </a>
    })
}

#[component]
pub(super) async fn export_menu(cx: &Cx, id: i64) -> topcoat::Result<impl View> {
    use crate::exports::Format;
    Ok(view! {
        cx =>
        <details class="export-menu" data-menu=(true)>
            <summary class="button button-secondary">"↗ Export"</summary>
            <nav class="menu-options" aria-label="Export this recipe">
                for (format, label) in [
                    (Format::Text, "Plain text"),
                    (Format::Markdown, "Markdown"),
                    (Format::Pdf, "PDF"),
                ] {
                    <a
                        href=(href!(
                            super::downloads::export_one,
                            RecipeId(id),
                            ExportFormat(format.as_str()),
                        ))
                    >
                        (label)
                    </a>
                }
            </nav>
        </details>
    })
}

#[component]
pub(super) async fn snackbar(message: &str) -> topcoat::Result<impl View> {
    Ok(view! {
        <div class="snackbar" data-snackbar=(true) role="status" aria-live="polite">
            <span>(message)</span>
            <button
                type="button"
                data-snackbar-dismiss=(true)
                aria-label="Dismiss notification"
            >
                "×"
            </button>
        </div>
    })
}
