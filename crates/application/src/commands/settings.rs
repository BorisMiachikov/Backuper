use domain::DomainResult;

use crate::context::AppContext;

pub async fn get(ctx: &AppContext, key: &str) -> DomainResult<Option<String>> {
    ctx.settings.get(key).await
}

pub async fn set(ctx: &AppContext, key: &str, value_json: &str) -> DomainResult<()> {
    ctx.settings.set(key, value_json).await
}
