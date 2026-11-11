use super::*;

#[page("/")]
pub(super) async fn home(cx: &Cx) -> topcoat::Result<impl View> {
    let user = match current_web_user(cx).await? {
        Some(user) => user,
        None => {
            return Err(redirect(
                if db(cx, |a| a.needs_setup()).await.map_err(server_error)? {
                    "/setup"
                } else {
                    "/login"
                },
            )
            .into());
        }
    };
    let ListOptions {
        search,
        category,
        sort,
        limit,
        offset,
    } = ListQuery::read(cx)?;
    let sort = sort.as_str();
    let (q, cat) = (search.clone(), category.clone());
    let page = db(cx, move |a| a.recipe_page(&q, &cat, sort, limit, offset))
        .await
        .map_err(server_error)?;
    let from = list_url(cx, &search, &category, sort, limit, offset);
    Ok(shell(
        cx,
        "Recipes".into(),
        Child::new(view! {
            cx =>
            <h1 class="sr-only">"Recipes"</h1>
            <form
                class="collection-controls"
                id="recipe-filters"
                method="get"
                action=(href!(crate::web::recipes::home))
            >
                <input type="hidden" name="limit" value=(limit.to_string()) />
                <div class="search-tools">
                    <div class="search-bar">
                        <label class="sr-only" for="recipe-search">
                            "Search names and descriptions"
                        </label>
                        <span class="search-icon" aria-hidden="true">"⌕"</span>
                        <input
                            id="recipe-search"
                            type="search"
                            name="q"
                            value=(search.as_str())
                            placeholder="Search names and descriptions…"
                        />
                        <kbd>"/"</kbd>
                    </div>
                    <label class="sort-control" for="recipe-sort">
                        <span>"Sort"</span>
                        <select id="recipe-sort" name="sort">
                            <option value="az" selected=(sort == "az")>
                                "Title A–Z"
                            </option>
                            <option value="time" selected=(sort == "time")>
                                "Quickest first"
                            </option>
                        </select>
                    </label>
                    <button
                        class="button button-secondary search-submit"
                        type="submit"
                        name="category"
                        value=(category.as_str())
                    >
                        "Search"
                    </button>
                </div>
                <div class="filters" role="group" aria-label="Filter by category">
                    <button
                        class="chip"
                        type="submit"
                        name="category"
                        value=""
                        aria-pressed=(category.is_empty().to_string())
                    >
                        "All"
                    </button>
                    for cat in &page.categories {
                        <button
                            class="chip"
                            type="submit"
                            name="category"
                            value=(cat.as_str())
                            aria-pressed=((category == *cat).to_string())
                        >
                            (cat.as_str())
                        </button>
                    }
                </div>
            </form>
            <p
                class="sr-only"
                id="results-announcement"
                role="status"
                aria-live="polite"
                aria-atomic="true"
            ></p>
            <div id="recipe-results" data-list-url=(from.as_str())>
                <p class="results-meta">
                    (page.total)
                    (if page.total == 1 { " recipe" } else { " recipes" })
                    if offset > 0 && !page.items.is_empty() {
                        " · Showing "
                        (offset.saturating_add(1))
                        "–"
                        (offset.saturating_add(page.items.len()))
                    }
                </p>
                if page.items.is_empty() {
                    <section class="empty-state">
                        <h2>"No recipes found"</h2>
                        <p class="muted">
                            "Try another name or description, or clear your filters."
                        </p>
                        <a
                            class="button button-secondary"
                            href=(href!(crate::web::recipes::home))
                        >
                            "Clear search"
                        </a>
                        if user.is_admin {
                            <a
                                class="text-link"
                                href=(href!(crate::web::settings::folders::folders_settings))
                            >
                                "Manage recipe folders →"
                            </a>
                        }
                    </section>
                } else {
                    <section class="recipe-list" aria-label="Recipes">
                        for recipe in &page.items {
                            recipe_row(recipe: recipe, from: &from)
                        }
                    </section>
                }
                if offset > 0 || offset.saturating_add(page.items.len()) < page.total {
                    <nav class="pagination" aria-label="Recipe pages">
                        if offset > 0 {
                            <a
                                class="button button-secondary"
                                href=(list_url(
                                    cx,
                                    &search,
                                    &category,
                                    sort,
                                    limit,
                                    offset.saturating_sub(limit),
                                ))
                            >
                                "← Previous"
                            </a>
                        }
                        if offset.saturating_add(page.items.len()) < page.total {
                            <a
                                class="button button-secondary"
                                href=(list_url(
                                    cx,
                                    &search,
                                    &category,
                                    sort,
                                    limit,
                                    offset.saturating_add(limit),
                                ))
                            >
                                "Next →"
                            </a>
                        }
                    </nav>
                }
            </div>
        }),
        true,
    ))
}

#[page("/recipes/{recipe_id}")]
pub(super) async fn recipe_page(cx: &Cx) -> topcoat::Result<impl View> {
    let _user = require_web_user(cx).await?;
    let id = *path_param::<RecipeId>(cx)?;
    let Some(recipe) = db(cx, move |a| a.recipe(id)).await.map_err(server_error)? else {
        return Err(not_found().into());
    };
    Ok(shell(
        cx,
        recipe.data.title.clone(),
        Child::new(view! {
            cx =>
            <article class="detail">
                <header class="detail-header">
                    <div class="detail-title-row">
                        <h1>(recipe.data.title.as_str())</h1>
                        export_menu(id: recipe.id)
                    </div>
                    <div class="tag-row">
                        for tag in recipe
                            .data
                            .categories
                            .iter()
                            .chain(recipe.data.cuisines.iter()) {
                            <span class="tag">(tag.as_str())</span>
                        }
                    </div>
                    if !recipe.data.description.is_empty() {
                        <p class="lead">(recipe.data.description.as_str())</p>
                    }
                    <p class="byline muted">
                        if !recipe.data.author.is_empty() {
                            "From "
                            (recipe.data.author.as_str())
                            "’s kitchen"
                        }
                        if !recipe.data.author.is_empty()
                            && !recipe.data.r#yield.is_empty() {
                            " · "
                        }
                        (recipe.data.r#yield.as_str())
                    </p>
                </header>
                if recipe.data.prep_time.is_some()
                    || recipe.data.cook_time.is_some()
                    || recipe.data.total_time.is_some() {
                    <div class="recipe-times" aria-label="Recipe timing">
                        if let Some(minutes) = recipe.data.prep_time {
                            <span>
                                "Prep "
                                (format_minutes(minutes))
                            </span>
                        }
                        if let Some(minutes) = recipe.data.cook_time {
                            <span>
                                "Cook "
                                (format_minutes(minutes))
                            </span>
                        }
                        if let Some(minutes) = recipe.data.total_time {
                            <span>
                                "Total "
                                (format_minutes(minutes))
                            </span>
                        }
                    </div>
                }
                <hr class="detail-rule" />
                <div class="recipe-body">
                    if !recipe.data.ingredients.is_empty() {
                        <section class="detail-section">
                            <h2>"Ingredients"</h2>
                            if !recipe.data.r#yield.is_empty() {
                                <p class="muted serving">(recipe.data.r#yield.as_str())</p>
                            }
                            <ul class="ingredients">
                                for ingredient in recipe
                                    .data
                                    .ingredients
                                    .lines()
                                    .filter(|l| !l.trim().is_empty()) {
                                    <li>(ingredient)</li>
                                }
                            </ul>
                        </section>
                    }
                    if !recipe.data.instructions.is_empty() {
                        <section class="detail-section">
                            <h2>"Method"</h2>
                            <ol class="instructions">
                                for instruction in &recipe.data.instructions {
                                    <li>(instruction.as_str())</li>
                                }
                            </ol>
                        </section>
                    }
                </div>
                if !recipe.data.notes.is_empty() {
                    <section class="panel notes">
                        <h2>"Notes"</h2>
                        for note in &recipe.data.notes {
                            <p>(note.as_str())</p>
                        }
                    </section>
                }
            </article>
        }),
        true,
    ))
}
