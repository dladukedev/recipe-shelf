use super::*;

#[derive(Deserialize)]
pub(super) struct ApiCredentials {
    username: String,
    password: String,
}
#[route(POST "/api/auth/login")]
pub(super) async fn api_login(
    cx: &Cx,
    Json(input): Json<ApiCredentials>,
) -> topcoat::Result<Json<crate::core::Login>> {
    let mut attempt = login_attempt(cx).map_err(|_| too_many_requests(300))?;
    let username = input.username;
    let password = input.password;
    match db(cx, move |a| a.login(&username, &password)).await {
        Ok(login) => {
            attempt.finish(true);
            Ok(Json(login))
        }
        Err(error) if error.is::<crate::domain_error::DomainError>() => {
            attempt.finish(false);
            Err(unauthorized().into())
        }
        Err(error) => Err(server_error(error)),
    }
}
#[route(POST "/api/auth/logout")]
pub(super) async fn api_logout(cx: &Cx) -> topcoat::Result<Json<serde_json::Value>> {
    let Some(token) = auth_token(cx) else {
        return Err(unauthorized().into());
    };
    db(cx, move |a| a.logout(&token))
        .await
        .map_err(server_error)?;
    clear_session(cx);
    Ok(Json(serde_json::json!({"ok": true})))
}
#[route(GET "/api/recipes")]
pub(super) async fn api_recipes(cx: &Cx) -> topcoat::Result<Json<Vec<Recipe>>> {
    let _user = api_user(cx).await?;
    let ListOptions {
        search: q,
        limit,
        offset,
        ..
    } = ListQuery::read(cx)?;
    let rows = db(cx, move |a| a.recipes(&q, limit, offset))
        .await
        .map_err(server_error)?;
    Ok(Json(rows))
}
#[route(GET "/api/recipes/{api_recipe_id}")]
pub(super) async fn api_recipe(cx: &Cx) -> topcoat::Result<Json<Recipe>> {
    let _user = api_user(cx).await?;
    let id = *path_param::<ApiRecipeId>(cx)?;
    match db(cx, move |a| a.recipe(id)).await.map_err(server_error)? {
        Some(recipe) => Ok(Json(recipe)),
        None => Err(not_found().into()),
    }
}
