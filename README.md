# tc-fetch

Fetch artifacts from taskcluster.

This is a small utility application designed to download and fetch
logs from [TaskCluster](https://taskcluster.net/)-based CI systems.

In particular it's built for use with [Mozilla's
CI](https://treeherder.mozilla.org) and
[web-platform-tests](https://github.com/web-platform/tests) CI.

## Command line usage

```
tcfetch [--out-dir <path>] [--artifact-name <name>] [--filter-jobs <expression>]* <repo> <commit>
```

By default tcfetch is configured to fetch web-platform-tests results
in wptreport format.

Valid `repo` names are:

* `mozilla-central`, `mozilla-beta`, `autoland`, `try` - Mozilla
  repositories hosted on [hg.mozilla.org](https://hg.mozilla.org).
* `wpt` - The [web-platform-tests](https://github.com/web-platform/tests) repository.

`commit` must be the hash of a commit in the corresponding
repository. For Mozilla repositories the minimum commit prefix is 12
characters. For web-platform-tests, anything non-ambiguous should
work.

`--out-dir` - The path to put the downloaded artifact files.

`--artifact-name` - The name of the artifact to download (currently
implemented as a suffix match on the full path).

`--filter-jobs` - A filter string used to select the task names to
include. This is a string that's interpreted as a regex. If the string
starts with `!`, any matching jobs are excluded. If the string starts
with `^` (after removing any `!`), it's used as a regexp against the
full task name, otherwise it's used as a substring match.

For example to fetch all Firefox logs from web-platform-tests commit
0f123ad and put them in a directory called `logs`:

```
tcfetch --out-dir logs --filter-jobs '-firefox-' wpt 0f123ad
```
