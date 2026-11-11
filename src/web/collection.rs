use super::*;
use crate::exports::Format;

fn selected_format(cx: &Cx) -> topcoat::Result<Format> {
    let value = &topcoat::router::query_params::<SelectionQuery>(cx)?
        .fields
        .0
        .format;
    if value.is_empty() {
        Ok(Format::Markdown)
    } else {
        Format::parse(value).map_err(|_| bad_request("invalid format").into())
    }
}

#[page("/exports")]
pub(super) async fn collection_export(cx: &Cx) -> topcoat::Result<impl View> {
    let user = require_web_user(cx).await?;
    let format = selected_format(cx)?;
    let recipe_id = selected_recipe(cx)?;
    let (recipes, rendered) = db(cx, move |app| {
        let recipes = app.export_recipes()?;
        let templates = app.export_templates()?;
        let rendered = preview::render(
            format.template(&templates),
            preview::sample(&recipes, recipe_id)?,
        )?;
        Ok((recipes, rendered))
    })
    .await
    .map_err(preview::http_error)?;
    let selected_id = recipe_id.or_else(|| recipes.first().map(|recipe| recipe.id));
    let format = format.as_str();
    Ok(shell(
        cx,
        "Export".into(),
        Child::new(view! {
            cx =>
            <h1 class="sr-only">"Export recipes"</h1>
            <div class="two-column">
                <section class="panel export-panel">
                    <h2>"All recipes"</h2>
                    <p class="muted">
                        (recipes.len())
                        (if recipes.len() == 1 { " recipe" } else { " recipes" })
                    </p>
                    <form
                        id="collection-export"
                        method="get"
                        action=(href!(select_collection_export))
                        class="stack-form"
                    >
                        <fieldset>
                            <legend>"File format"</legend>
                            <div class="format-options">
                                for (key, label, hint) in [
                                    ("text", "Plain text", ".txt · Simple & universal"),
                                    ("markdown", "Markdown", ".md · Ready for your notes"),
                                    ("pdf", "PDF", ".pdf · Ready to print"),
                                ] {
                                    <label>
                                        <input
                                            type="radio"
                                            name="format"
                                            value=(key)
                                            checked=(key == format)
                                        />
                                        <strong>(label)</strong>
                                        <small class="muted">(hint)</small>
                                    </label>
                                }
                            </div>
                        </fieldset>
                        <button
                            class="button button-primary export-download"
                            type="submit"
                        >
                            "↓ Download all recipes"
                        </button>
                    </form>
                </section>
                <aside class="panel export-preview" aria-label="Export preview">
                    <h2>"Template preview"</h2>
                    <div class="preview-recipe-form">
                        <label for="export-preview-recipe">"Preview recipe"</label>
                        <select
                            id="export-preview-recipe"
                            name="recipe"
                            form="collection-export"
                            disabled=(recipes.is_empty())
                        >
                            if recipes.is_empty() {
                                <option value="">"No recipes available"</option>
                            }
                            for recipe in &recipes {
                                <option
                                    value=(recipe.id.to_string())
                                    selected=(Some(recipe.id) == selected_id)
                                >
                                    (recipe.data.title.as_str())
                                </option>
                            }
                        </select>
                        <button
                            class="button button-secondary preview-refresh"
                            id="export-update-preview"
                            form="collection-export"
                            type="submit"
                            formaction=(href!(collection_export))
                        >
                            "Update preview"
                        </button>
                    </div>
                    <pre id="export-preview" class="template-preview">
                        (rendered.preview.as_str())
                    </pre>
                    <p
                        id="export-preview-message"
                        class="muted"
                        role="status"
                        aria-live="polite"
                    >
                        (rendered
                            .error
                            .as_deref()
                            .unwrap_or(
                                if rendered.truncated {
                                    "Preview shortened. Exports use the full template."
                                } else if format == "pdf" {
                                    "PDF preview shows the template’s HTML."
                                } else {
                                    "Preview using your saved template."
                                },
                            ))
                    </p>
                    if user.is_admin {
                        <a
                            id="customize-export-template"
                            class="text-link"
                            href=(href!(
                                crate::web::settings::templates::templates_settings,
                            ).query([("format", format)]))
                        >
                            "Customize export templates →"
                        </a>
                    }
                </aside>
            </div>
        }),
        true,
    ))
}

#[route(GET "/exports/preview")]
async fn saved_preview(cx: &Cx) -> topcoat::Result<Json<preview::Preview>> {
    let _user = web_user(cx).await?;
    let format = selected_format(cx)?;
    let recipe_id = selected_recipe(cx)?;
    let result = db(cx, move |app| {
        let recipe =
            match recipe_id {
                Some(id) => Some(app.recipe(id)?.ok_or(
                    crate::domain_error::DomainError::NotFound("recipe not found"),
                )?),
                None => app
                    .recipe_page("", "", "az", 1, 0)?
                    .items
                    .into_iter()
                    .next(),
            };
        preview::render(format.template(&app.export_templates()?), recipe.as_ref())
    })
    .await
    .map_err(preview::http_error)?;
    Ok(Json(result))
}

#[page("/exports/download")]
pub(super) async fn select_collection_export(cx: &Cx) -> topcoat::Result<()> {
    let _user = web_user(cx).await?;
    let format = topcoat::router::query_params::<SelectionQuery>(cx)?
        .fields
        .0
        .format
        .clone();
    if Format::parse(&format).is_err() {
        return Err(bad_request("invalid format").into());
    }
    Err(see_other(href!(super::downloads::export_all, ExportFormat(&format)).resolve(cx)).into())
}
