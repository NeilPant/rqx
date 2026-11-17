# Policy

A policy is a configuration file that many repositories share. Configuration is
composed by walking up from the file being scanned and stops at the repository
boundary, so a file outside the repository cannot be picked up that way. A policy
provides a shared file that each repository names explicitly.

```yaml
# services/billing/rqx.yaml
policy: ../../platform/rqx-policy.yaml

thresholds:
  max_params: 2
```

```yaml
# platform/rqx-policy.yaml
thresholds:
  cognitive_complexity: 12
  max_params: 4

gate:
  fail_on: warning

locked:
  keys: [thresholds.cognitive_complexity, gate.fail_on]
```

The policy is an ordinary configuration file. It may set anything a project can,
including `rule` (custom rules) and `overrides`. The path is relative to the file that names it, or
absolute. `RQX_POLICY` overrides the path, so that a CI job can impose a policy.

## It sits under the project, not over it

A service may tighten any key the policy did not lock. In the example above,
`max_params` is 4 in the policy and 2 in the service, and the service's value, 2,
applies.

```
  cognitive complexity            12 rqx-policy.yaml · locked
  max parameters                   2 rqx.yaml
```

`rqx config` names the file each setting came from, per key and not per table, so a
value supplied by the policy is not attributed to the project's own file.

Presets sit lower in the precedence. Everything a file `extends`, in the project or in
the policy itself, is applied beneath every file rqx reads. A preset named by a
service therefore cannot override the policy or change a key the policy locks. See
[presets.md](presets.md).

## Locked keys

A key in `locked.keys` cannot be given a different value by anything above the
policy: the project's configuration, an entry of `overrides`, or `--level`.

```
Error: `thresholds.cognitive_complexity` is locked to 12 by ../../platform/rqx-policy.yaml;
       services/billing/rqx.yaml sets 25.
```

Restating the policy's own value is allowed, so a service can record the standard it
meets.

A locked key is fixed to the policy's value. It is not a "may not be loosened"
constraint. Some keys have no loosening direction (`rules.disabled` is one), so the
same rule applies to every locked key: the policy's value is the effective value.

Locking `rules.disabled` to `["magic-number"]` therefore also prevents a service from
disabling any other rule. The policy cannot set a minimum list that services may
extend. A project that needs that has to leave the key unlocked.

Only the policy may lock. A `locked` mapping in a project's own configuration is
refused.

## What it is for

A policy applies one set of thresholds and gate settings to every repository that
names it. A single repository does not need one.

A policy can be combined with a [baseline](baseline.md) per repository. The policy
defines the standard, the baseline records the findings each service currently has
against it, and the ratchet fails the gate when the number of findings grows.
