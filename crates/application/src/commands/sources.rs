use domain::{DomainResult, Source};
use uuid::Uuid;

use crate::context::AppContext;

pub async fn list(ctx: &AppContext) -> DomainResult<Vec<Source>> {
    ctx.sources.list().await
}

pub async fn upsert(ctx: &AppContext, source: &Source) -> DomainResult<()> {
    source.validate()?;
    ctx.sources.upsert(source).await?;
    let _ = ctx.events.send(domain::DomainEvent::SourceChanged { source_id: source.id });
    Ok(())
}

pub async fn delete(ctx: &AppContext, id: Uuid) -> DomainResult<()> {
    ctx.sources.delete(id).await?;
    let _ = ctx.events.send(domain::DomainEvent::SourceChanged { source_id: id });
    Ok(())
}
