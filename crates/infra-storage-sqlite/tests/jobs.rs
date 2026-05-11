use std::path::PathBuf;

use domain::{
    ArchiveConfig, ExcludeRules, Job, JobRepository, JobRun, JobRunStatus, RetentionPolicy,
    RunTrigger, Source, SourceKind, SourceRepository,
};
use infra_storage_sqlite::{SqliteJobRepository, SqliteSourceRepository};
use time::OffsetDateTime;
use uuid::Uuid;

async fn fixtures() -> (SqliteJobRepository, SqliteSourceRepository, Source) {
    let pool = infra_storage_sqlite::open_in_memory().await.unwrap();
    let src_repo = SqliteSourceRepository::new(pool.clone());
    let job_repo = SqliteJobRepository::new(pool);

    let source = Source::new(SourceKind::Folder, "Test Source", PathBuf::from(r"D:\data"));
    src_repo.upsert(&source).await.unwrap();
    (job_repo, src_repo, source)
}

fn sample_job(source_id: Uuid, name: &str) -> Job {
    let now = OffsetDateTime::now_utc();
    Job {
        id: Uuid::now_v7(),
        source_id,
        name: name.into(),
        enabled: true,
        archive: ArchiveConfig::default(),
        retention: RetentionPolicy::default(),
        exclude: ExcludeRules::default(),
        pre_cmd: None,
        post_cmd: None,
        priority: 0,
        targets: Vec::new(),
        created_at: now,
        updated_at: now,
    }
}

fn sample_run(job_id: Uuid, status: JobRunStatus) -> JobRun {
    JobRun {
        id: Uuid::now_v7(),
        job_id,
        trigger: RunTrigger::Manual,
        status,
        started_at: OffsetDateTime::now_utc(),
        finished_at: None,
        bytes_in: 0,
        bytes_out: 0,
        files_count: 0,
        archive_path: None,
        error_msg: None,
        host: "test-host".into(),
        attempt: 0,
    }
}

#[tokio::test]
async fn job_upsert_get_list_delete() {
    let (jobs, _src, source) = fixtures().await;
    let job = sample_job(source.id, "Nightly Backup");

    jobs.upsert(&job).await.unwrap();

    let got = jobs.get(job.id).await.unwrap().expect("present");
    assert_eq!(got.name, "Nightly Backup");
    assert_eq!(got.source_id, source.id);
    assert!(got.enabled);

    let listed = jobs.list().await.unwrap();
    assert_eq!(listed.len(), 1);

    jobs.delete(job.id).await.unwrap();
    assert!(jobs.get(job.id).await.unwrap().is_none());
}

#[tokio::test]
async fn upsert_preserves_archive_config_json() {
    let (jobs, _src, source) = fixtures().await;
    let mut job = sample_job(source.id, "X");
    job.archive.compression_level = 9;
    job.archive.name_template = "custom-{yyyy}.{ext}".into();
    job.exclude.masks = vec!["*.tmp".into(), "1Cv8Log/*".into()];

    jobs.upsert(&job).await.unwrap();

    let got = jobs.get(job.id).await.unwrap().unwrap();
    assert_eq!(got.archive.compression_level, 9);
    assert_eq!(got.archive.name_template, "custom-{yyyy}.{ext}");
    assert_eq!(got.exclude.masks, vec!["*.tmp", "1Cv8Log/*"]);
}

#[tokio::test]
async fn insert_then_update_run_lifecycle() {
    let (jobs, _src, source) = fixtures().await;
    let job = sample_job(source.id, "X");
    jobs.upsert(&job).await.unwrap();

    let mut run = sample_run(job.id, JobRunStatus::Pending);
    jobs.insert_run(&run).await.unwrap();

    run.status = JobRunStatus::Running;
    jobs.update_run(&run).await.unwrap();

    run.status = JobRunStatus::Success;
    run.finished_at = Some(OffsetDateTime::now_utc());
    run.bytes_out = 1_048_576;
    run.files_count = 42;
    run.archive_path = Some(r"D:\Backups\x.zip".into());
    jobs.update_run(&run).await.unwrap();

    let history = jobs.list_runs(job.id, 10).await.unwrap();
    assert_eq!(history.len(), 1);
    let saved = &history[0];
    assert_eq!(saved.status, JobRunStatus::Success);
    assert_eq!(saved.bytes_out, 1_048_576);
    assert_eq!(saved.files_count, 42);
    assert_eq!(saved.archive_path.as_deref(), Some(r"D:\Backups\x.zip"));
    assert!(saved.finished_at.is_some());
}

#[tokio::test]
async fn list_runs_orders_by_started_desc() {
    let (jobs, _src, source) = fixtures().await;
    let job = sample_job(source.id, "X");
    jobs.upsert(&job).await.unwrap();

    let mut older = sample_run(job.id, JobRunStatus::Success);
    older.started_at = OffsetDateTime::now_utc() - time::Duration::hours(2);
    jobs.insert_run(&older).await.unwrap();

    let newer = sample_run(job.id, JobRunStatus::Success);
    jobs.insert_run(&newer).await.unwrap();

    let history = jobs.list_runs(job.id, 10).await.unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].id, newer.id);
    assert_eq!(history[1].id, older.id);
}

#[tokio::test]
async fn mark_running_as_interrupted_affects_only_running_and_pending() {
    let (jobs, _src, source) = fixtures().await;
    let job = sample_job(source.id, "X");
    jobs.upsert(&job).await.unwrap();

    let pending = sample_run(job.id, JobRunStatus::Pending);
    let running = sample_run(job.id, JobRunStatus::Running);
    let done = sample_run(job.id, JobRunStatus::Success);
    jobs.insert_run(&pending).await.unwrap();
    jobs.insert_run(&running).await.unwrap();
    jobs.insert_run(&done).await.unwrap();

    let affected = jobs.mark_running_as_interrupted().await.unwrap();
    assert_eq!(affected, 2);

    let runs = jobs.list_runs(job.id, 10).await.unwrap();
    let by_id: std::collections::HashMap<Uuid, JobRunStatus> =
        runs.into_iter().map(|r| (r.id, r.status)).collect();
    assert_eq!(by_id[&pending.id], JobRunStatus::Interrupted);
    assert_eq!(by_id[&running.id], JobRunStatus::Interrupted);
    assert_eq!(by_id[&done.id], JobRunStatus::Success);
}

#[tokio::test]
async fn delete_source_cascades_to_jobs_and_runs() {
    let (jobs, src_repo, source) = fixtures().await;
    let job = sample_job(source.id, "X");
    jobs.upsert(&job).await.unwrap();
    let run = sample_run(job.id, JobRunStatus::Success);
    jobs.insert_run(&run).await.unwrap();

    src_repo.delete(source.id).await.unwrap();

    assert!(jobs.get(job.id).await.unwrap().is_none());
    let history = jobs.list_runs(job.id, 10).await.unwrap();
    assert!(history.is_empty());
}
