# @shieldbattery/stimpack

**@shieldbattery/stimpack:** Rust-based Node.js native module for launching processes and injecting DLLs into them

## Local development

To do local development and testing, you can utilize `npm link` to work off of your local copy:

```sh
cd path/to/stimpack
yarn link
cd path/to/shieldbattery/app
yarn link @shieldbattery/stimpack
```

To return to the published version:

```sh
cd path/to/stimpack
yarn unlink
cd path/to/shieldbattery/app
yarn unlink @shieldbattery/stimpack
yarn install --check-files
```

## Cancelling an owned launch

Create an owner before starting injection so cancellation can reach a process even
while `launch` is pending:

```js
const { createLaunchOwner, launch, terminateLaunchOwner } = require('@shieldbattery/stimpack')

function startOwnedLaunch(args) {
  const owner = createLaunchOwner()
  return {
    launched: launch(args, owner),
    stop: () => terminateLaunchOwner(owner),
  }
}
```

Observe both promises: `launched` can reject after cancellation, and `stop()` can
reject if exit is not confirmed. The stop method is available while launch is pending.

An owner belongs to exactly one launch and cannot be reused after cancellation.
Termination uses the handle returned by that launch's `CreateProcessW`; no PID or
process-name lookup is involved. A process created after cancellation is rejected
while suspended, before injection resumes it. Repeated termination is safe.

For an existing process, termination waits up to five seconds for its exit to be
confirmed. A failure rejects the promise: keep the owner to retry and do not treat
that rejection as proof of exit. Cancellation does not provide an `onEnd` callback;
callers should try their normal graceful shutdown before forcing termination.

The one-argument `launch(args)` API remains available. An explicit `undefined`
owner also preserves that behavior. Calls without an owner do not gain cancellation.

`yarn test` includes real Windows child-process fixtures. The ignored `hung_fixture`
test is a subprocess entry used by those tests; do not run it directly. After
building `native/index.node`, run `node test/launch-owner.cjs` to check the JS API.

## Release integration

Consumers should use the signed prebuild from a published package version. The
release workflow builds `native/index.node`, signs it, tags it as a prebuild and
publishes the selected version. Local source builds do not establish signing or
release readiness. This API change does not require consumers to compile Rust
once an updated signed package is available.
