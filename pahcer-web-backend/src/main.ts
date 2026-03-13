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

program.command('run')
  .description('Run pahcer via the web server')
  .allowUnknownOption()
  .argument('[args...]', 'Arguments to pass to pahcer')
  .option('-p, --port <number>', 'Port of the running pahcer-web server', '10432')
  .action(async (args, options) => {
    const port = options.port;
    const url = `http://localhost:${port}/api/run`;
    
    try {
        const response = await fetch(url, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ args })
        });

        if (!response.ok) {
            console.error(`Server returned error: ${response.status} ${response.statusText}`);
            const text = await response.text();
            if (text) console.error(text);
            process.exit(1);
        }

        if (!response.body) {
            console.error('No response body from server');
            process.exit(1);
        }

        // Handle streaming response (NDJSON)
        // @ts-ignore
        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = '';

        while (true) {
            const { done, value } = await reader.read();
            if (done) break;
            
            buffer += decoder.decode(value, { stream: true });
            const lines = buffer.split('\n');
            buffer = lines.pop() || ''; // Keep the last incomplete line

            for (const line of lines) {
                if (!line.trim()) continue;
                try {
                    const msg = JSON.parse(line);
                    if (msg.type === 'stdout') {
                         process.stdout.write(msg.data);
                    } else if (msg.type === 'stderr') {
                         process.stderr.write(msg.data);
                    } else if (msg.type === 'exit') {
                         process.exit(msg.code);
                    }
                } catch (e) {
                    // Ignore non-JSON lines or parse errors
                }
            }
        }
    } catch (e) {
        console.error('Failed to connect to pahcer-web server.');
        console.error(`Ensure the server is running on port ${port} (use 'pahcer-web ui').`);
        console.error(`Error: ${e}`);
        process.exit(1);
    }
  });

program.parse(process.argv);
