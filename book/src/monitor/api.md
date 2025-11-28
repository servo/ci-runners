# Monitor API

<style>
  var { color: var(--sidebar-fg); background: var(--sidebar-bg); padding: 0 0.25em; margin: 0 0.125em; }
  dt { margin-top: 0.5em; font-weight: bold; }
  h3 ._method { margin-right: 1em; }
</style>

- [Reserving runners](#reserving-runners)
  - [<span class="_method">POST</span> /select-runner](#POST/select-runner)
  - [<span class="_method">POST</span> /profile/<var>profile_key</var>/take](#POST/profile/.../take)
  - [<span class="_method">POST</span> /profile/<var>profile_key</var>/take/<var>count</var>](#POST/profile/.../take/...)
- [Runner internals](#runner-internals)
  - [<span class="_method">GET</span> /github-jitconfig](#GET/github-jitconfig)
  - [<span class="_method">GET</span> /boot](#GET/boot)
- [Dashboard internals](#dashboard-internals)
  - [<span class="_method">GET</span> /dashboard.html](#GET/dashboard.html)
  - [<span class="_method">GET</span> /dashboard.json](#GET/dashboard.json)
  - [<span class="_method">GET</span> /profile/<var>profile_key</var>/screenshot.png](#GET/profile/.../screenshot.png)
  - [<span class="_method">GET</span> /runner/<var>runner_id</var>/screenshot.png](#GET/runner/.../screenshot.png)
  - [<span class="_method">GET</span> /runner/<var>runner_id</var>/screenshot/now](#GET/runner/.../screenshot/now)
- [Policy overrides (EXPERIMENTAL)](#policy-overrides-experimental)
  - [<span class="_method">GET</span> /policy/override](#GET/policy/override)
  - [<span class="_method">POST</span> /policy/override](#POST/policy/override)
  - [<span class="_method">DELETE</span> /policy/override](#DELETE/policy/override)

## Notes about endpoints

Some of the endpoints below **require the monitor API token**.
Requests to these endpoints need to prove you know that token:

```
Authorization: Bearer <monitor API token>
```

Some of the endpoints below **may require cranking the backend**.
While requests to other endpoints are guaranteed to be cheap, requests to *these* endpoints can be a bit expensive when successful, because they’re processed *one at a time* in a backend known as the **monitor thread**.

The monitor thread interacts with external resources like the GitHub API and the hypervisor, and it runs a loop that looks like the pseudocode below:

```rust,noplayground
loop {
    if registrations_last_updated.elapsed()
        > MONITOR_DOT_TOML.api_cache_timeout
    {
        registrations = github_api::list_registered_runners();
        registrations_last_updated = Instant::now();
    }
    guests = hypervisor_api::list_guests();
    hypervisor_api::take_screenshots(guests);
    hypervisor_api::check_ipv4_addresses(guests);

    if let Some((request, response_tx)) = monitor_request_rx
        .recv_timeout(MONITOR_DOT_TOML.monitor_poll_interval)
    {
        response_tx.send(match request {
            // ...
        });
    }
}
```

## Reserving runners

The recommended way to reserve runners is to use the **tokenless API** ([<span class="_method">POST</span> /select-runner](#POST/select-runner)), which uses a temporary artifact to prove that the request is genuine and authorised.
This allows self-hosted runners to be used in [`pull_request`](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#pull_request) runs (rather than only [`pull_request_target`](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#pull_request_target)), and in workflows that do not have access to secrets.

Alternatively you can use the monitor API token, which for workflows means you will need to define it as a secret like `${{ secrets.MONITOR_API_TOKEN }}`.

### <span class="_method">POST</span> /select-runner <br>— Reserve one runner for a job using an artifact { #POST/select-runner }

- **May require cranking the backend**

<dl>
<dt>?<var>unique_id</var> (required; <span class="_type">UUIDv4</span>)</dt>
<dd>uniquely identifies this job in its friendly name, even if the same workflow is called twice in the workflow call tree</dd>
<dt>?<var>qualified_repo</var> (required; <code>&lt;user>/&lt;repo></code>)</dt>
<dd>the repository running this job</dd>
<dt>?<var>run_id</var> (required; <span class="_type">number</span>)</dt>
<dd>the workflow run id of this job</dd>
</dl>

### <span class="_method">POST</span> /profile/<var>profile_key</var>/take <br>— Reserve one runner for a job using the monitor API token { #POST/profile/.../take }

- **Requires monitor API token**
- **May require cranking the backend**
- **Response:** application/json — `{"id", "runner"}` | `null`

<dl>
<dt><var>profile_key</var> (string)</dt>
<dd>what kind of runner to take</dd>
<dt>?<var>unique_id</var> (required; <span class="_type">UUIDv4</span>)</dt>
<dd>uniquely identifies this job in its friendly name, even if the same workflow is called twice in the workflow call tree</dd>
<dt>?<var>qualified_repo</var> (required; <code>&lt;user>/&lt;repo></code>)</dt>
<dd>the repository running this job</dd>
<dt>?<var>run_id</var> (required; <span class="_type">number</span>)</dt>
<dd>the workflow run id of this job</dd>
</dl>

### <span class="_method">POST</span> /profile/<var>profile_key</var>/take/<var>count</var> <br>— Reserve runners for a set of jobs using the monitor API token { #POST/profile/.../take/... }

- **Requires monitor API token**
- **May require cranking the backend**
- **Response:** application/json — `[{"id", "runner"}]` | `null`

<dl>
<dt><var>profile_key</var> (string)</dt>
<dd>what kind of runners to take</dd>
<dt><var>count</var> (number)</dt>
<dd>how many runners to take</dd>
<dt>?<var>unique_id</var> (required; <span class="_type">UUIDv4</span>)</dt>
<dd>uniquely identifies these jobs in their friendly names, even if the same workflow is called twice in the workflow call tree</dd>
<dt>?<var>qualified_repo</var> (required; <code>&lt;user>/&lt;repo></code>)</dt>
<dd>the repository running these jobs</dd>
<dt>?<var>run_id</var> (required; <span class="_type">number</span>)</dt>
<dd>the workflow run id of these jobs</dd>
</dl>

## Runner internals

### <span class="_method">GET</span> /github-jitconfig <br>— Get the ephemeral runner token for this runner { #GET/github-jitconfig }

- **May require cranking the backend**
- **Response:** application/json

### <span class="_method">GET</span> /boot <br>— Get the boot script for this runner { #GET/boot }

- **May require cranking the backend**
- **Response:** text/plain

## Dashboard internals

### <span class="_method">GET</span> /dashboard.html <br>— Get the rendered contents of the dashboard for live updates { #GET/dashboard.html }

- **Response:** text/html

### <span class="_method">GET</span> /dashboard.json <br>— Get a machine-readable version of the contents of the dashboard { #GET/dashboard.json }

- **Response:** application/json

### <span class="_method">GET</span> /profile/<var>profile_key</var>/screenshot.png <br>— Get the last cached screenshot of a rebuild guest { #GET/profile/.../screenshot.png }

- **Response:** image/png

### <span class="_method">GET</span> /runner/<var>runner_id</var>/screenshot.png <br>— Get the last cached screenshot of a runner guest { #GET/runner/.../screenshot.png }

- **Response:** image/png

### <span class="_method">GET</span> /runner/<var>runner_id</var>/screenshot/now <br>— Take a screenshot of a runner guest immediately { #GET/runner/.../screenshot/now }

- **May require cranking the backend**
- **Response:** image/png

## Policy overrides (EXPERIMENTAL)

Policy overrides provide rudimentary support for autoscaling, implemented as part of Servo’s effort to self-host [WPT](https://web-platform-tests.org) runs ([#21](https://github.com/servo/ci-runners/issues/21)).
The design has several unsolved problems, and should not be used.

They allow us to dynamically reconfigure a server’s runner targets to meet the needs of a workflow.
This can be useful if that workflow is huge and parallel, and you want to divert as much of your concurrent runner capacity as possible to it.

### <span class="_method">GET</span> /policy/override <br>— Get the current policy override { #GET/policy/override }

### <span class="_method">POST</span> /policy/override <br>— Initiate a new policy override { #POST/policy/override }

- **Requires monitor API token**
- **Response:** application/json — `{"<<profile_key>>": <count>}`

<dl>
<dt>?<var>&lt;profile_key></var>=<var>count</var> (required; string/number pairs)</dt>
<dd>how many runners to target for each profile key</dd>
</dl>

### <span class="_method">DELETE</span> /policy/override <br>— Cancel the current policy override { #DELETE/policy/override }

- **Requires monitor API token**
- **Response:** application/json — `{"<<profile_key>>": <count>}`
