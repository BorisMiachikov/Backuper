use std::path::PathBuf;

use domain::{Source, SourceKind, SourceRepository};
use infra_storage_sqlite::SqliteSourceRepository;

async fn pool() -> sqlx::SqlitePool {
    infra_storage_sqlite::open_in_memory()
        .await
        .expect("in-memory db must open")
}

fn sample(name: &str) -> Source {
    let mut s = Source::new(SourceKind::Folder, name, PathBuf::from(r"D:\data"));
    s.description = Some("test source".into());
    s.tags = vec!["prod".into(), "1c".into()];
    s
}

#[tokio::test]
async fn upsert_then_get_returns_same_source() {
    let pool = pool().await;
    let repo = SqliteSourceRepository::new(pool);
    let src = sample("Бухгалтерия");
    repo.upsert(&src).await.expect("upsert");

    let got = repo.get(src.id).await.expect("get").expect("present");
    assert_eq!(got.id, src.id);
    assert_eq!(got.name, src.name);
    assert_eq!(got.kind, SourceKind::Folder);
    assert_eq!(got.path, src.path);
    assert!(got.enabled);
    assert_eq!(got.description.as_deref(), Some("test source"));
    let mut got_tags = got.tags.clone();
    got_tags.sort();
    assert_eq!(got_tags, vec!["1c".to_string(), "prod".to_string()]);
}

#[tokio::test]
async fn list_sorted_by_name_ci() {
    let pool = pool().await;
    let repo = SqliteSourceRepository::new(pool);
    for n in ["zeta", "alpha", "Mike"] {
        repo.upsert(&sample(n)).await.unwrap();
    }
    let list = repo.list().await.unwrap();
    let names: Vec<&str> = list.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["alpha", "Mike", "zeta"]);
}

#[tokio::test]
async fn upsert_replaces_tags() {
    let pool = pool().await;
    let repo = SqliteSourceRepository::new(pool);
    let mut src = sample("X");
    src.tags = vec!["a".into(), "b".into()];
    repo.upsert(&src).await.unwrap();

    src.tags = vec!["c".into()];
    repo.upsert(&src).await.unwrap();

    let got = repo.get(src.id).await.unwrap().unwrap();
    assert_eq!(got.tags, vec!["c".to_string()]);
}

#[tokio::test]
async fn delete_removes_source_and_tags() {
    let pool = pool().await;
    let repo = SqliteSourceRepository::new(pool.clone());
    let src = sample("To delete");
    repo.upsert(&src).await.unwrap();

    repo.delete(src.id).await.unwrap();
    assert!(repo.get(src.id).await.unwrap().is_none());

    // Теги тоже должны исчезнуть (ON DELETE CASCADE).
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM source_tags WHERE source_id = ?1")
        .bind(src.id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn roundtrip_one_c_server_variant() {
    let pool = pool().await;
    let repo = SqliteSourceRepository::new(pool);
    let mut src = Source::new(
        SourceKind::OneCServer {
            server: "1c-srv01.local".into(),
            ref_base: "Производство".into(),
            cluster_port: Some(1541),
        },
        "Server base",
        PathBuf::from("ref"),
    );
    src.tags = vec!["server".into()];
    repo.upsert(&src).await.unwrap();

    let got = repo.get(src.id).await.unwrap().unwrap();
    match got.kind {
        SourceKind::OneCServer { server, ref_base, cluster_port } => {
            assert_eq!(server, "1c-srv01.local");
            assert_eq!(ref_base, "Производство");
            assert_eq!(cluster_port, Some(1541));
        }
        other => panic!("expected OneCServer, got {other:?}"),
    }
}
