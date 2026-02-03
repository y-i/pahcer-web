#!/usr/bin/env node
import { Command } from 'commander';
import { startServer } from './server.js';


const program = new Command();

program
  .name('pahcer-web')
  .description('Web UI for pahcer')
  .version('0.0.0');

program.command('ui')
  .description('Start the pahcer-web UI server')
  .option('-p, --port <number>', 'Port to listen on', '10432')
  .option('--no-build', 'Skip building the frontend')
  .action(startServer);

program.parse(process.argv);
