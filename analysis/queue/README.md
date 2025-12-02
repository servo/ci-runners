queue analysis
==============

these scripts analyse usage of the global queueing feature ([#69](https://github.com/servo/ci-runners/pull/69)).

to find jobs that are allowed to reserve self-hosted runners, and see if they actually used self-hosted runners:

```
$ ./find-self-hosted-jobs.sh
```

to find jobs that have benefitted from global queueing:

```
$ ./find-effective-queueing.sh
```

to find jobs that have suffered delays due to GitHub-hosted runner capacity:

```
$ ./find-jobs-with-large-delays.sh
```

## 2025-12-02

global queueing landed just over a week ago, in [servo#40852](https://github.com/servo/servo/pull/40852) (workflow run [19658917259](https://github.com/servo/servo/actions/runs/19658917259)).

there were 717 non-trivial workflow runs in [servo/servo](https://github.com/servo/servo) since then, where “non-trivial” means ≥ 120 seconds from creation to last update.
those workflow runs include 655 job runs that were allowed to reserve self-hosted runners, of which 611 actually used self-hosted runners, which is a 93.2% success rate.

exactly one job run has benefitted from global queueing, defined as a runner-select runtime of ≥ 30 seconds:

```json
{
  "duration": 94,
  "html_url": "https://github.com/servo/servo/actions/runs/19721369467/job/56504457956",
  "run_url": "https://api.github.com/repos/servo/servo/actions/runs/19721369467",
  "name": "Linux (Unit Tests) / Linux / runner-select"
}
```

27 runner-select job runs have suffered delays due to GitHub-hosted runner capacity, defined as a created-to-started time of ≥ 30 seconds:

```json
{"delay":84,"run_id":19848761522,"id":56871277039,"name":"Linux (Unit Tests) / Linux / runner-select"}
{"delay":109,"run_id":19848761522,"id":56871277085,"name":"Lint / Lint / runner-select"}
{"delay":48,"run_id":19848247850,"id":56869747146,"name":"Upload nightly (Linux) / runner-select"}
{"delay":48,"run_id":19848247850,"id":56869747160,"name":"Upload nightly (macOS) / runner-select"}
{"delay":111,"run_id":19848247850,"id":56869747163,"name":"Upload nightly (Windows) / runner-select"}
{"delay":143,"run_id":19833660866,"id":56825675217,"name":"Linux (Unit Tests) / Linux / runner-select"}
{"delay":82,"run_id":19833660866,"id":56825675333,"name":"Lint / Lint / runner-select"}
{"delay":247,"run_id":19829593251,"id":56812082759,"name":"Linux (Unit Tests) / Linux / runner-select"}
{"delay":31,"run_id":19829593251,"id":56812082774,"name":"Lint / Lint / runner-select"}
{"delay":233,"run_id":19816705770,"id":56769644157,"name":"Linux (Unit Tests) / Linux / runner-select"}
{"delay":233,"run_id":19816705770,"id":56769644256,"name":"Lint / Lint / runner-select"}
{"delay":138,"run_id":19808694851,"id":56747062596,"name":"Linux (Unit Tests) / Linux / runner-select"}
{"delay":107,"run_id":19808694851,"id":56747062639,"name":"Lint / Lint / runner-select"}
{"delay":34,"run_id":19808684096,"id":56747048456,"name":"Linux (Unit Tests) / Linux / runner-select"}
{"delay":98,"run_id":19808684096,"id":56747048461,"name":"Lint / Lint / runner-select"}
{"delay":124,"run_id":19808220994,"id":56745841851,"name":"Linux (Unit Tests) / Linux / runner-select"}
{"delay":165,"run_id":19808220994,"id":56745841868,"name":"Lint / Lint / runner-select"}
{"delay":346,"run_id":19756060380,"id":56607872467,"name":"Linux (Unit Tests) / Linux / runner-select"}
{"delay":288,"run_id":19756060380,"id":56607872479,"name":"Lint / Lint / runner-select"}
{"delay":122,"run_id":19738422692,"id":56556173245,"name":"Linux (Unit Tests) / Linux / runner-select"}
{"delay":128,"run_id":19738422692,"id":56556173457,"name":"Lint / Lint / runner-select"}
{"delay":192,"run_id":19730262507,"id":56529768640,"name":"Linux (Unit Tests) / Linux / runner-select"}
{"delay":238,"run_id":19730262507,"id":56529768986,"name":"Lint / Lint / runner-select"}
{"delay":30,"run_id":19707397241,"id":56459377754,"name":"Lint / Lint / runner-select"}
{"delay":41,"run_id":19707397241,"id":56459377763,"name":"Linux (Unit Tests) / Linux / runner-select"}
{"delay":40,"run_id":19659306396,"id":56302403841,"name":"Linux (Unit Tests) / Linux / runner-select"}
{"delay":31,"run_id":19659306396,"id":56302403898,"name":"Lint / Lint / runner-select"}
```

all but two of those happened after we alleviated pressure on GitHub-hosted runner capacity by runner-timeout jobs in [servo#40876](https://github.com/servo/servo/pull/40876) (workflow run [19670059700](https://github.com/servo/servo/actions/runs/19670059700)).
