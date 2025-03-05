extern crate tcfetch as tcfetch_rs;
use pyo3::exceptions::PyOSError;
use pyo3::prelude::*;
use std::env;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
struct Error(tcfetch_rs::Error);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::convert::From<tcfetch_rs::Error> for Error {
    fn from(err: tcfetch_rs::Error) -> Error {
        Error(err)
    }
}

impl std::convert::From<Error> for PyErr {
    fn from(err: Error) -> PyErr {
        PyOSError::new_err(err.0.to_string())
    }
}

#[pyclass(frozen)]
pub struct TaskDownloadData {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub path: PathBuf,
    #[pyo3(get)]
    pub run_id: Option<String>,
}

impl TaskDownloadData {
    fn from_download(task: tcfetch_rs::taskcluster::TaskGroupTask, download_path: PathBuf) -> Self {
        TaskDownloadData {
            id: task.status.taskId,
            name: task.task.metadata.name,
            path: download_path,
            run_id: task
                .task
                .extra
                .get("test-setting")
                .and_then(|x| x.get("_hash"))
                .and_then(|x| x.as_str())
                .map(|x| x.to_owned()),
        }
    }
}

#[pyfunction]
#[pyo3(signature = (branch, commit, taskcluster_base=None))]
pub fn check_complete(
    branch: &str,
    commit: &str,
    taskcluster_base: Option<&str>,
) -> PyResult<bool> {
    Ok(tcfetch_rs::check_complete(taskcluster_base, branch, commit).map_err(Error::from)?)
}

#[pyfunction]
#[pyo3(signature = (branch, commit, artifact_name=None, taskcluster_base=None, task_filters=None, check_complete=false, out_dir=None, compress=false))]
pub fn download_artifacts(
    branch: &str,
    commit: &str,
    artifact_name: Option<&str>,
    taskcluster_base: Option<&str>,
    task_filters: Option<Vec<String>>,
    check_complete: bool,
    out_dir: Option<&str>,
    compress: bool,
) -> PyResult<Vec<TaskDownloadData>> {
    let cur_dir = env::current_dir().expect("Invalid working directory");
    let out_path: PathBuf = if let Some(dir) = out_dir {
        cur_dir.join(dir)
    } else {
        cur_dir
    };
    if !out_path.is_dir() {
        return Err(Error::from(tcfetch_rs::Error::String(format!(
            "{} is not a directory",
            out_path.display()
        )))
        .into());
    }

    let task_filters = task_filters
        .map(|filters| {
            filters
                .iter()
                .map(|filter_str| tcfetch_rs::TaskFilter::new(filter_str).map_err(Error::from))
                .collect::<Result<Vec<_>, Error>>()
        })
        .transpose()?;

    Ok(tcfetch_rs::download_artifacts(
        taskcluster_base,
        branch,
        commit,
        task_filters,
        artifact_name,
        check_complete,
        &out_path,
        compress,
    )
    .map_err(Error::from)?
    .into_iter()
    .map(|(task, path)| TaskDownloadData::from_download(task, path))
    .collect())
}

/// Download artifacts from Taskcluster.
#[pymodule]
fn tcfetch(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(download_artifacts, m)?)?;
    m.add_function(wrap_pyfunction!(check_complete, m)?)?;
    m.add_class::<TaskDownloadData>()?;
    Ok(())
}
