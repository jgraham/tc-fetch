use clap::{Arg, ArgAction, Command};
use std::env;
use std::path::PathBuf;
use tcfetch::{download_artifacts, Error, Result, TaskFilter};

fn parse_args() -> Command {
    Command::new("Taskcluster artifact fetcher")
        .arg(
            Arg::new("check_complete")
                .long("check-complete")
                .required(false)
                .action(ArgAction::SetTrue)
                .help("Check if there are any pending wpt jobs and exit with code 1 if there are"),
        )
        .arg(
            Arg::new("compress")
                .long("compress")
                .required(false)
                .action(ArgAction::SetTrue)
                .help("Compress output as zstd"),
        )
        .arg(
            Arg::new("out_dir")
                .long("out-dir")
                .required(false)
                .help("Directory in which to put output files"),
        )
        .arg(
            Arg::new("artifact_name")
                .long("artifact-name")
                .help("Artifact name to fetch"),
        )
        .arg(
            Arg::new("taskcluster_url")
                .long("taskcluster-url")
                .help("Base url of the taskcluster instance"),
        )
        .arg(
            Arg::new("filter_re")
                .long("filter-jobs")
                .action(ArgAction::Append)
                .help("Regex to filter task names. If this starts with ! then a matching task is excluded. If it start with ^ (after removing any !) the remaining regex is applied to the start of the task string, otherwise any prefix is allowed. Tasks must match all given filters."),
        )
        .arg(
            Arg::new("repo")
                .required(true)
                .index(1)
                .help("Repo in which jobs ran"),
        )
        .arg(
            Arg::new("commit")
                .required(true)
                .index(2)
                .help("Commit hash"),
        )
}

fn main() -> Result<()> {
    let matches = parse_args().get_matches();
    let repo = matches.get_one::<String>("repo").unwrap();
    let commit = matches.get_one::<String>("commit").unwrap();
    let taskcluster_base = matches.get_one::<String>("taskcluster_url");
    let artifact_name = matches.get_one::<String>("artifact_name");
    let task_filter_strs = matches.get_many::<String>("filter_re");
    let check_complete = matches.get_flag("check_complete");
    let compress = matches.get_flag("compress");

    let cur_dir = env::current_dir().expect("Invalid working directory");
    let out_dir: PathBuf = if let Some(dir) = matches.get_one::<String>("out_dir") {
        cur_dir.join(dir)
    } else {
        cur_dir
    };
    if !out_dir.is_dir() {
        return Err(Error::String(format!(
            "{} is not a directory",
            out_dir.display()
        )));
    }

    let task_filters = task_filter_strs
        .map(|filters| {
            filters
                .map(|filter| TaskFilter::new(filter))
                .collect::<Result<Vec<TaskFilter>>>()
        })
        .transpose()?;

    download_artifacts(
        taskcluster_base.map(|x| x.as_str()),
        repo,
        commit,
        task_filters,
        artifact_name.map(|x| x.as_str()),
        check_complete,
        &out_dir,
        compress,
    )?;

    Ok(())
}
