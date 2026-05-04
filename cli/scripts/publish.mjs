import { spawnSync } from 'node:child_process'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptsDir = fileURLToPath(new URL('.', import.meta.url))
const cliDir = resolve(scriptsDir, '..')
const stageScript = resolve(scriptsDir, 'stage-frontend.mjs')

const forwardedArgs = []
const seenFlags = new Set()

for (const arg of process.argv.slice(2)) {
  if (arg === '--dry-run') {
    if (seenFlags.has(arg)) continue
    seenFlags.add(arg)
  }

  forwardedArgs.push(arg)
}

const stageResult = spawnSync(process.execPath, [stageScript], {
  cwd: cliDir,
  stdio: 'inherit',
})

if (stageResult.status !== 0) {
  process.exit(stageResult.status ?? 1)
}

const publishResult = spawnSync('cargo', ['publish', ...forwardedArgs], {
  cwd: cliDir,
  stdio: 'inherit',
})

process.exit(publishResult.status ?? 1)