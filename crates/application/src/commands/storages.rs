use domain::{DomainResult, StorageDescriptor};
use uuid::Uuid;

use crate::context::AppContext;

pub async fn list(ctx: &AppContext) -> DomainResult<Vec<StorageDescriptor>> {
    ctx.storages.list().await
}

pub async fn upsert(ctx: &AppContext, desc: &StorageDescriptor) -> DomainResult<()> {
    ctx.storages.upsert(desc).await?;
    let _ = ctx.events.send(domain::DomainEvent::StorageChanged { storage_id: desc.id });
    Ok(())
}

pub async fn delete(ctx: &AppContext, id: Uuid) -> DomainResult<()> {
    ctx.storages.delete(id).await?;
    let _ = ctx.events.send(domain::DomainEvent::StorageChanged { storage_id: id });
    Ok(())
}
