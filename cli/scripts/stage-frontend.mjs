import { cpSync, existsSync, mkdirSync, rmSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptsDir = fileURLToPath(new URL('.', import.meta.url))
const cliDir = resolve(scriptsDir, '..')
const sourceDir = resolve(cliDir, '../frontend/dist')
const sourceIndex = resolve(sourceDir, 'index.html')
const destinationDir = resolve(cliDir, 'frontend-assets')

if (!existsSync(sourceIndex)) {
  console.error(`frontend build output is missing: ${sourceIndex}`)
  process.exit(1)
}

rmSync(destinationDir, { force: true, recursive: true })
mkdirSync(destinationDir, { recursive: true })
cpSync(sourceDir, destinationDir, { recursive: true })
