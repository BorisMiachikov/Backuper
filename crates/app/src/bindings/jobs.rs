use domain::Job;
use slint::SharedString;

pub fn job_to_row(job: &Job) -> crate::JobRow {
    crate::JobRow {
        id: SharedString::from(job.id.to_string()),
        name: SharedString::from(job.name.as_str()),
        last_run: SharedString::from("—"),
        status: SharedString::from(if job.enabled { "idle" } else { "disabled" }),
        progress: 0.0,
    }
}
