const assert = require('node:assert/strict')
const { randomUUID } = require('node:crypto')
const os = require('node:os')
const path = require('node:path')
const native = require('../native/index.node')

async function main() {
  const args = {
    appPath: path.join(os.tmpdir(), `stimpack-missing-${randomUUID()}.exe`),
    args: '',
    currentDir: os.tmpdir(),
    dllPath: 'unused.dll',
    dllFunc: 'Unused',
    logCallback: () => {},
  }
  const owner = native.createLaunchOwner()
  await native.terminateLaunchOwner(owner)
  await native.terminateLaunchOwner(owner)
  await assert.rejects(native.launch(args, owner), /Process launch cancelled/)
  assert.throws(() => native.launch(args, {}), TypeError)
  await assert.rejects(native.launch(args), /Failed to create process/)
  await assert.rejects(native.launch(args, undefined), /Failed to create process/)
  console.log('JS ownership, cancellation and legacy launch checks passed')
}

main().catch(error => {
  console.error(error)
  process.exitCode = 1
})
