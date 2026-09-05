# Policy

Forty repositories, one standard. Config composes by walking up from the file being
scanned and stops at the repository boundary, so a platform team has nowhere to put
the thing every service should agree on. A policy is that file.

```toml
# services/billing/RQX.toml
policy = "../../platform/rqx-policy.toml"

[thresholds]
max_params = 2
```

```toml
# platform/rqx-policy.toml
[thresholds]
cognitive_complexity = 12
max_params = 4

[gate]
fail_on = "warning"

[locked]
keys = ["thresholds.cognitive_complexity", "gate.fail_on"]
```

The policy is a normal config file. It may set anything a project can, including
`[[rule]]` and `[[override]]`. The path is relative to the file that names it, or
absolute; `RQX_POLICY` overrides it, for a CI job that has to impose one.

## It sits under the project, not over it

A service may **tighten** anything the policy did not lock. Above, `max_params` is 4
in the policy and 2 in the service, and 2 wins — the service is holding itself to a
higher standard, which is not something to refuse.

```
  cognitive complexity            12 rqx-policy.toml · locked
  max parameters                   2 RQX.toml
```

`rqx config` names the file each setting actually came from, per key rather than per
table, so a value the policy supplied is not attributed to the project's own file.

## Locked keys

A key in `[locked] keys` may not be given a different value by anything above the
policy — not the project's config, not an `[[override]]`, not `--level`:

```
Error: `thresholds.cognitive_complexity` is locked to 12 by ../../platform/rqx-policy.toml;
       services/billing/RQX.toml sets 25.
```

Restating the policy's own value is allowed, so a service can write down the standard
it is meeting.

Note that this is *locked*, not *may not be loosened*. Loosening is not a thing that
can be decided for every key — `rules.disabled` has no direction — so the rule that
holds everywhere is the simple one: the policy's value is the value.

That has a consequence worth knowing. Locking `rules.disabled` to `["magic-number"]`
stops a service disabling anything else as well. If you want a floor rather than an
exact list, lock nothing and rely on review.

Only the policy may lock. A `[locked]` table in a project's own config is refused —
a repository locking itself against the platform is the wrong way round.

## What it is for

A policy is worth reaching for when the answer to "why is this threshold 25 here and
12 there" should be "it should not be". A single repository does not need one; a
platform team publishing a standard to forty does.

Pair it with a [baseline](baseline.md) per repository: the policy says what the
standard is, the baseline says how far each service currently is from it, and the
ratchet says the distance may only shrink.
