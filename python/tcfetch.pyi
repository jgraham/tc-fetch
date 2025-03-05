from typing import Optional

class TaskDownloadData:
    id: str
    name: str
    path: str
    run_id: Optional[str]

def check_complete(
    branch: str, commit: str, taskcluster_base: Optional[str] = None
) -> bool: ...
def download_artifacts(
    branch: str,
    commit: str,
    artifact_name: Optional[str] = None,
    taskcluster_base: Optional[str] = None,
    task_filters: Optional[str] = None,
    check_complete: bool = False,
    out_dir: Optional[str] = None,
    compress: bool = False
) -> list[TaskDownloadData]: ...
