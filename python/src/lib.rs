extern crate fetchlogs as fetchlogs_rs;
use pyo3::exceptions::PyOSError;
use pyo3::prelude::*;
use std::env;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
struct Error(fetchlogs_rs::Error);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::convert::From<fetchlogs_rs::Error> for Error {
    fn from(err: fetchlogs_rs::Error) -> Error {
        Error(err)
    }
}

impl std::convert::From<Error> for PyErr {
    fn from(err: Error) -> PyErr {
        PyOSError::new_err(err.0.to_string())
    }
}

#[pyfunction]
#[pyo3(signature = (branch, commit, artifact_name=None, taskcluster_base=None, task_filters=None, check_complete=false, out_dir=None))]
fn download_artifacts(
    branch: &str,
    commit: &str,
    artifact_name: Option<&str>,
    taskcluster_base: Option<&str>,
    task_filters: Option<Vec<&str>>,
    check_complete: bool,
    out_dir: Option<&str>,
) -> PyResult<Vec<PathBuf>> {
    let cur_dir = env::current_dir().expect("Invalid working directory");
    let out_path: PathBuf = if let Some(dir) = out_dir {
        cur_dir.join(dir)
    } else {
        cur_dir
    };
    if !out_path.is_dir() {
        return Err(Error::from(fetchlogs_rs::Error::String(format!(
            "{} is not a directory",
            out_path.display()
        )))
        .into());
    }

    let task_filters = task_filters
        .map(|filters| {
            filters
                .iter()
                .map(|filter_str| {
                    fetchlogs_rs::TaskFilter::new(filter_str).map_err(|err| Error::from(err))
                })
                .collect::<Result<Vec<_>, Error>>()
        })
        .transpose()?;

    Ok(fetchlogs_rs::download_artifacts(
        taskcluster_base,
        branch,
        commit,
        task_filters,
        artifact_name,
        check_complete,
        &out_path,
    )
    .map_err(|err| Error::from(err))?)
}

/// Download artifacts from Taskcluster.
#[pymodule]
fn fetchlogs(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(download_artifacts, m)?)?;
    Ok(())
}
