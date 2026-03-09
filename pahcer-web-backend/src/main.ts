#!/usr/bin/env node
import { Command } from 'commander';
import { startServer } from './server.js';
import { fileURLToPath } from 'node:url';
import { dirname } from 'node:path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const program = new Command();

program
  .name('pahcer-web')
  .description('Web UI for pahcer')
  .version('0.0.0')
  .option('-C, --directory <path>', 'Base directory for pahcer and .pahcer-web metadata', __dirname);

program.command('ui')
  .description('Start the pahcer-web UI server')
  .option('-p, --port <number>', 'Port to listen on', '10432')
  .option('--no-build', 'Skip building the frontend')
  .action((options) => {
    const globalOptions = program.opts();
    startServer({ ...options, directory: globalOptions.directory });
  });

program.parse(process.argv);
