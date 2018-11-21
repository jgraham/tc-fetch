# fetchlogs

Fetch wpt logs from job results for pushes listed on Treeherder.

## Getting started

First, clone this repository and make sure prerequisites are installed.

### Prerequisites

You need to have Cargo and Rust installed

For MacOS and Linux systems, run this command:
```shell
$ curl -sSf https://static.rust-lang.org/rustup.sh | sh
```
For Windows, download and run [rustup-init.exe](https://win.rustup.rs/).

## Usage

The simplest usage is to fetch results from a push to a branch:
```shell
$ cargo run try b60ef0011594
```
This will fetch raw logs for all web-platform-test jobs on push `b60ef0011594`
on the `try` repository to the current working directory.

The `wptreport` style logs are smaller, you can download those by specifying
`--log–type wptreport`:
```shell
$ cargo run try b60ef0011594 --log-type wptreport
```

If you'd rather not clutter the current directory with the downloaded logs, you
can specify a target directory with `--out-dir <desired/path>`:
```shell
$ cargo run try b60ef0011594 --out-dir ~/wptlogs/
```
