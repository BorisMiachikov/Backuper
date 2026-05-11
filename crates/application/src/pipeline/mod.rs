//! Стадии бэкапа: Prepare → PreHook → AcquireLock → Collect → Archive
//! → Verify → Upload → Retention → PostHook → Cleanup → Persist.
//!
//! Stage 1.2 — реализована минимальная сквозная стадия: создание `JobRun` в БД,
//! сбор файлов через `walkdir`, упаковка в `.zip` (без компрессии, без пароля),
//! sha-256 verify, запись финального статуса. Хранилище — локальная папка
//! `%LOCALAPPDATA%\Backuper\archives\<job-name>\`.
//! Этого достаточно, чтобы UI начал получать реальные runs; полноценные стадии
//! (1С, облака, retention, hooks) добавляются на Stage 2-4.

use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use domain::{DomainEvent, Job, JobRun, JobRunStatus, JobTrigger, PipelineStage, StageProgress};
use sha2::{Digest, Sha256};
use thiserror::Error;
use time::OffsetDateTime;
use tracing::{debug, info, warn};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::context::AppContext;

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("source not found in database")]
    SourceMissing,
    #[error("retryable: {0}")]
    Retryable(String),
    #[error("fatal: {0}")]
    Fatal(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
}

impl PipelineError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Retryable(_) | Self::Io(_))
    }
}

pub async fn run(
    ctx: &AppContext,
    job: &Job,
    trigger: &JobTrigger,
) -> Result<JobRun, PipelineError> {
    let host = sysinfo::System::host_name().unwrap_or_else(|| "unknown".into());
    let mut run = JobRun {
        id: trigger.run_id,
        job_id: job.id,
        trigger: trigger.trigger,
        status: JobRunStatus::Running,
        started_at: OffsetDateTime::now_utc(),
        finished_at: None,
        bytes_in: 0,
        bytes_out: 0,
        files_count: 0,
        archive_path: None,
        error_msg: None,
        host,
        attempt: trigger.attempt,
    };

    // Insert исходный run (status=running).
    if let Err(e) = ctx.jobs.insert_run(&run).await {
        return Err(PipelineError::Fatal(format!("insert_run: {e}")));
    }
    emit_started(ctx, job.id, run.id);

    let outcome = backup_pipeline(ctx, job, &mut run).await;

    match outcome {
        Ok(()) => {
            run.status = JobRunStatus::Success;
            run.finished_at = Some(OffsetDateTime::now_utc());
        }
        Err(ref e) => {
            run.status = JobRunStatus::Failed;
            run.finished_at = Some(OffsetDateTime::now_utc());
            run.error_msg = Some(e.to_string());
        }
    }

    if let Err(e) = ctx.jobs.update_run(&run).await {
        warn!(error = %e, "update_run failed");
    }
    emit_finished(ctx, &run);

    match outcome {
        Ok(()) => Ok(run),
        Err(e) => Err(e),
    }
}

async fn backup_pipeline(
    ctx: &AppContext,
    job: &Job,
    run: &mut JobRun,
) -> Result<(), PipelineError> {
    let source = ctx
        .sources
        .get(job.source_id)
        .await
        .map_err(|e| PipelineError::Fatal(format!("source repo: {e}")))?
        .ok_or(PipelineError::SourceMissing)?;

    info!(job_id = %job.id, source_id = %source.id, "pipeline: collecting files");
    emit_stage(ctx, job.id, run.id, PipelineStage::Collect, 0.0);

    let files = collect_files(&source.path)?;
    run.files_count = files.len() as u64;
    run.bytes_in = files.iter().map(|(_, size)| *size).sum();
    debug!(files = run.files_count, bytes_in = run.bytes_in, "pipeline: collected");

    emit_stage(ctx, job.id, run.id, PipelineStage::Archive, 0.0);
    let archive_path = build_archive_path(job, run.id)?;
    let archive_size = create_archive(&files, &source.path, &archive_path, |done, total| {
        let pct = if total > 0 { done as f32 / total as f32 } else { 0.0 };
        emit_stage(ctx, job.id, run.id, PipelineStage::Archive, pct);
    })?;
    run.bytes_out = archive_size;
    run.archive_path = Some(archive_path.to_string_lossy().into_owned());

    emit_stage(ctx, job.id, run.id, PipelineStage::Verify, 0.0);
    let _hash = sha256_of(&archive_path)?;
    emit_stage(ctx, job.id, run.id, PipelineStage::Verify, 1.0);

    info!(
        job_id = %job.id,
        bytes_in = run.bytes_in,
        bytes_out = run.bytes_out,
        files = run.files_count,
        archive = %archive_path.display(),
        "pipeline: success"
    );
    Ok(())
}

fn collect_files(root: &std::path::Path) -> Result<Vec<(PathBuf, u64)>, PipelineError> {
    if !root.exists() {
        return Err(PipelineError::Fatal(format!(
            "source path does not exist: {}",
            root.display()
        )));
    }
    let mut out = Vec::new();
    if root.is_file() {
        let size = std::fs::metadata(root)?.len();
        out.push((root.to_path_buf(), size));
        return Ok(out);
    }
    for entry in WalkDir::new(root).follow_links(false).into_iter().flatten() {
        if entry.file_type().is_file() {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            out.push((entry.into_path(), size));
        }
    }
    Ok(out)
}

fn build_archive_path(job: &Job, run_id: Uuid) -> Result<PathBuf, PipelineError> {
    let base = shared::paths::data_dir().join("archives").join(sanitize(&job.name));
    std::fs::create_dir_all(&base)?;
    let stamp = OffsetDateTime::now_utc()
        .format(&time::format_description::parse(
            "[year][month][day]-[hour][minute][second]",
        )
        .map_err(|e| PipelineError::Fatal(format!("format desc: {e}")))?)
        .unwrap_or_else(|_| "run".into());
    Ok(base.join(format!("{stamp}-{}.zip", &run_id.to_string()[..8])))
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn create_archive(
    files: &[(PathBuf, u64)],
    root: &std::path::Path,
    out: &std::path::Path,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<u64, PipelineError> {
    let file = File::create(out)?;
    let mut writer = zip::ZipWriter::new(file);
    let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(3));
    let total: u64 = files.iter().map(|(_, s)| *s).sum();
    let mut done: u64 = 0;

    for (path, _size) in files {
        let rel = path.strip_prefix(root).unwrap_or(path);
        let name = rel.to_string_lossy().replace('\\', "/");
        if path.is_dir() {
            writer.add_directory(name, opts)?;
            continue;
        }
        writer.start_file(name, opts)?;
        let mut input = File::open(path)?;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = input.read(&mut buf)?;
            if n == 0 {
                break;
            }
            use std::io::Write;
            writer.write_all(&buf[..n])?;
            done += n as u64;
            if done % (1 << 20) == 0 {
                on_progress(done, total);
            }
        }
    }
    writer.finish()?;
    on_progress(total, total);
    Ok(std::fs::metadata(out)?.len())
}

fn sha256_of(path: &std::path::Path) -> Result<String, PipelineError> {
    let mut hasher = Sha256::new();
    let mut f = File::open(path)?;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_lower(&hasher.finalize()))
}

fn hex_lower(bytes: &[u8]) -> String {
    const H: &[u8] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(H[(b >> 4) as usize] as char);
        s.push(H[(b & 0xf) as usize] as char);
    }
    s
}

fn emit_started(ctx: &AppContext, job_id: Uuid, run_id: Uuid) {
    let _ = ctx
        .events
        .send(DomainEvent::JobRunStarted(domain::JobRunStarted {
            run_id,
            job_id,
        }));
}

fn emit_finished(ctx: &AppContext, run: &JobRun) {
    let duration_ms = run
        .finished_at
        .map(|t| t - run.started_at)
        .map(|d| d.whole_milliseconds().max(0) as u64)
        .unwrap_or(0);
    let _ = ctx.events.send(DomainEvent::JobRunFinished(domain::JobRunFinished {
        run_id: run.id,
        job_id: run.job_id,
        status: run.status,
        bytes_out: run.bytes_out,
        duration_ms,
        error: run.error_msg.clone(),
    }));
}

fn emit_stage(ctx: &AppContext, job_id: Uuid, run_id: Uuid, stage: PipelineStage, percent: f32) {
    let _ = ctx.events.send(DomainEvent::StageProgress(StageProgress {
        run_id,
        job_id,
        stage,
        percent,
        bytes_done: 0,
        bytes_total: None,
    }));
}
