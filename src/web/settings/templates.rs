use super::*;

#[page("/settings/templates")]
pub(in crate::web) async fn templates_settings(cx: &Cx) -> topcoat::Result<impl View> {
    let user = require_web_admin(cx).await?;
    let selection = &topcoat::router::query_params::<SelectionQuery>(cx)?
        .fields
        .0;
    let format = crate::exports::Format::parse(&selection.format)
        .unwrap_or(crate::exports::Format::Markdown);
    let recipe_id = selected_recipe(cx)?;
    let (templates, recipes, rendered) = db(cx, move |app| {
        let templates = app.export_templates()?;
        let recipes = app.export_recipes()?;
        let rendered = preview::render(
            format.template(&templates),
            preview::sample(&recipes, recipe_id)?,
        )?;
        Ok((templates, recipes, rendered))
    })
    .await
    .map_err(preview::http_error)?;
    let selected_id = recipe_id.or_else(|| recipes.first().map(|recipe| recipe.id));
    let source = format.template(&templates).to_owned();
    let defaults = crate::exports::default_templates();
    let default_source = format.template(&defaults).to_owned();
    let format = format.as_str();
    Ok(view! {
        cx =>
        settings_frame(
            section: SettingsSection::Templates,
            user: &user,
            <section class="panel">
                <div class="panel-toolbar">
                    <nav class="format-tabs" aria-label="Template format">
                        for (key, label) in [
                            ("text", "Plain text"),
                            ("markdown", "Markdown"),
                            ("pdf", "PDF / HTML"),
                        ] {
                            <a
                                href=(href!(
                                    crate::web::settings::templates::templates_settings,
                                ).query(
                                    [
                                        ("format", key.to_owned()),
                                        (
                                            "recipe",
                                            selected_id.map(|id| id.to_string()).unwrap_or_default(),
                                        ),
                                    ],
                                ))
                                aria-current=(if key == format { "page" } else { "false" })
                            >
                                (label)
                            </a>
                        }
                    </nav>
                </div>
                <div class="template-layout">
                    <form
                        method="post"
                        action=(href!(crate::web::settings::templates::save_template))
                        class="template-editor"
                        data-template-form=(true)
                        data-owner=(user.id.to_string())
                    >
                        csrf_field()
                        <input type="hidden" name="format" value=(format) />
                        <input
                            type="hidden"
                            name="recipe"
                            value=(selected_id
                                .map(|id| id.to_string())
                                .unwrap_or_default())
                        />
                        <label for="template-source">
                            (if format == "pdf" {
                                "PDF HTML template"
                            } else if format == "text" {
                                "Plain text template"
                            } else {
                                "Markdown template"
                            })
                        </label>
                        <textarea
                            id="template-source"
                            name="source"
                            rows="19"
                            spellcheck="false"
                            data-default-source=(default_source.as_str())
                        >
                            (source.as_str())
                        </textarea>
                        <div class="button-row">
                            <button class="button button-primary" type="submit">
                                "Save template"
                            </button>
                            <button
                                class="button button-secondary"
                                type="button"
                                data-template-reset=(true)
                            >
                                "Reset to default"
                            </button>
                            <button
                                class="button button-secondary"
                                type="button"
                                data-template-discard=(true)
                            >
                                "Discard changes"
                            </button>
                        </div>
                        <p
                            class="muted"
                            id="template-status"
                            role="status"
                            aria-live="polite"
                        >
                            ""
                        </p>
                    </form>
                    <aside class="template-help">
                        <form
                            method="get"
                            action=(href!(templates_settings))
                            id="template-recipe-form"
                            class="preview-recipe-form"
                        >
                            <input type="hidden" name="format" value=(format) />
                            <label for="template-preview-recipe">
                                "Preview recipe"
                            </label>
                            <select
                                id="template-preview-recipe"
                                data-default-recipe=(recipes
                                    .first()
                                    .map(|recipe| recipe.id.to_string())
                                    .unwrap_or_default())
                                name="recipe"
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
                                type="submit"
                            >
                                "Update preview"
                            </button>
                        </form>
                        <pre id="template-preview" class="template-preview">
                            (rendered.preview.as_str())
                        </pre>
                        <p id="preview-message" role="status" aria-live="polite">
                            (rendered
                                .error
                                .as_deref()
                                .unwrap_or(
                                    if rendered.truncated {
                                        "Preview shortened. Exports use the full template."
                                    } else if format == "pdf" {
                                        "PDF preview shows the template’s HTML."
                                    } else {
                                        "Preview using the selected recipe."
                                    },
                                ))
                        </p>
                        <details>
                            <summary>"Template values"</summary>
                            <p>
                                "Customize exports with Mustache. Variables are escaped by default; triple braces insert raw HTML for PDF templates."
                            </p>
                            <p class="token-list">
                                "title · description · ingredients · ingredient_lines · instructions · steps · notes · categories · cuisines · author · yield · prep_time · cook_time · total_time · prep_minutes · cook_minutes · total_minutes · id · raw_toml · has_instructions"
                            </p>
                            <p>
                                "Loop arrays with {{#instructions}}…{{/instructions}}. PDF preview shows HTML source; export a recipe to see the generated document."
                            </p>
                        </details>
                    </aside>
                </div>
            </section>
        )
    })
}

#[derive(Deserialize)]
pub(in crate::web) struct TemplateForm {
    csrf: String,
    format: String,
    source: String,
    recipe: Option<i64>,
}
pub(in crate::web) fn template_error_page<'a>(
    cx: &'a Cx,
    format: String,
    source: String,
    saved: crate::core::ExportTemplates,
    owner: i64,
    message: String,
) -> impl View + 'a {
    let saved_source = match format.as_str() {
        "text" => saved.text,
        "markdown" => saved.markdown,
        _ => saved.pdf,
    };
    shell(
        cx,
        "Template error".to_owned(),
        Child::new(view! {
            cx =>
            (topcoat::router::StatusCode::BAD_REQUEST)
            <section class="narrow-card template-error">
                <p class="eyebrow">"Template not saved"</p>
                <h1>"Fix this template"</h1>
                <p class="notice notice-error" role="alert">(message)</p>
                <p class="muted">
                    "Your edited text is still here. The saved template remains unchanged until this version validates."
                </p>
                <form
                    method="post"
                    action=(href!(crate::web::settings::templates::save_template))
                    class="template-editor"
                    data-template-form=(true)
                    data-owner=(owner.to_string())
                >
                    csrf_field()
                    <input type="hidden" name="format" value=(format.clone()) />
                    <label for="template-source">"Edited template"</label>
                    <textarea
                        id="template-source"
                        name="source"
                        data-saved-source=(saved_source.as_str())
                        rows="14"
                        spellcheck="false"
                    >
                        (source.clone())
                    </textarea>
                    <button class="button button-primary" type="submit">
                        "Try saving again"
                    </button>
                    <p
                        class="muted"
                        id="template-status"
                        role="status"
                        aria-live="polite"
                    ></p>
                </form>
                <details class="template-help">
                    <summary>"Previously saved version"</summary>
                    <textarea
                        readonly=(true)
                        rows="10"
                        aria-label="Previously saved template"
                    >
                        (saved_source.clone())
                    </textarea>
                </details>
                <a
                    class="back-link"
                    href=(href!(crate::web::settings::templates::templates_settings))
                >
                    "← Templates"
                </a>
            </section>
        }),
        true,
    )
}
#[page(POST "/settings/templates")]
pub(in crate::web) async fn save_template(
    cx: &Cx,
    Form(form): Form<TemplateForm>,
) -> topcoat::Result<impl View> {
    require_csrf(cx, &form.csrf)?;
    let actor = require_admin(cx).await?;
    let existing = db(cx, |a| a.export_templates())
        .await
        .map_err(server_error)?;
    if crate::exports::Format::parse(&form.format).is_err() {
        return Ok(template_error_page(
            cx,
            form.format,
            form.source,
            existing,
            actor.id,
            "Choose text, markdown, or pdf as the template format.".to_owned(),
        ));
    }
    let validation_source = form.source.clone();
    let validation =
        tokio::task::spawn_blocking(move || crate::mustache::validate(&validation_source))
            .await
            .map_err(internal_server_error)?;
    if let Err(error) = validation {
        return Ok(template_error_page(
            cx,
            form.format,
            form.source,
            existing,
            actor.id,
            format!("Mustache validation: {error}"),
        ));
    }
    let format = form.format;
    let source = form.source;
    let mut params = vec![("format", format.clone()), ("saved", "1".to_owned())];
    if let Some(id) = form.recipe {
        params.push(("recipe", id.to_string()));
    }
    let location = href!(templates_settings).query(params).resolve(cx);
    db(cx, move |a| {
        a.save_export_template(&actor, &format, &source)
    })
    .await
    .map_err(server_error)?;
    Err(see_other(location).into())
}
#[route(POST "/settings/templates/preview")]
pub(in crate::web) async fn preview_template(
    cx: &Cx,
    Form(form): Form<TemplateForm>,
) -> topcoat::Result<Json<serde_json::Value>> {
    require_csrf(cx, &form.csrf)?;
    let _actor = require_admin(cx).await?;
    if crate::exports::Format::parse(&form.format).is_err() {
        return Err(bad_request("invalid format").into());
    }
    let result = db(cx, move |app| {
        let recipe =
            match form.recipe {
                Some(id) => Some(app.recipe(id)?.ok_or(
                    crate::domain_error::DomainError::NotFound("recipe not found"),
                )?),
                None => app
                    .recipe_page("", "", "az", 1, 0)?
                    .items
                    .into_iter()
                    .next(),
            };
        let rendered = preview::render(&form.source, recipe.as_ref())?;
        Ok(serde_json::to_value(rendered)?)
    })
    .await
    .map_err(preview::http_error)?;
    Ok(Json(result))
}
