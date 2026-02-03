import { fileURLToPath } from 'node:url';
import { Hono } from 'hono';
import { serve } from '@hono/node-server';
import { serveStatic } from '@hono/node-server/serve-static';
import { dirname, join, resolve, relative } from 'node:path';
import { spawn, execSync } from 'node:child_process';

import { Storage, type GlobalConfig, type LocalConfig } from './storage.js';
import { streamText } from 'hono/streaming';
import { writeFile, readFile } from 'node:fs/promises';


export async function startServer(options: any) {
    const storage = new Storage();

    const __filename = fileURLToPath(import.meta.url);
    const __dirname = dirname(__filename);
    const currentDir = __dirname;
    const backendRoot = resolve(currentDir, '..'); 
    const frontendDir = resolve(backendRoot, '../pahcer-web-frontend');
    const distDir = join(frontendDir, 'dist');
    
    if (options.build) {
      console.log(`Building frontend in ${frontendDir}...`);
      try {
        execSync('npm run build', { cwd: frontendDir, stdio: 'inherit' });
      } catch (e) {
        console.error("Failed to build frontend.");
        process.exit(1);
      }
    }

    const app = new Hono();
    const api = new Hono();

    // --- API Routes ---

    // Config
    api.get('/config', async (c) => {
      const global = await storage.getGlobalConfig();
      const local = await storage.getLocalConfig();
      return c.json({ global, local });
    });

    api.post('/config/global', async (c) => {
      const config = await c.req.json();
      await storage.saveGlobalConfig(config);
      return c.json({ success: true });
    });

    api.post('/config/local', async (c) => {
      const config = await c.req.json();
      await storage.saveLocalConfig(config);
      return c.json({ success: true });
    });

    // Jobs
    api.get('/jobs', async (c) => {
      const jobs = await storage.getJobs();
      return c.json(jobs);
    });

    // Visualizer
    api.post('/visualizer/download', async (c) => {
      const { url } = await c.req.json();
      if (!url) return c.json({ error: 'URL is required' }, 400);
      
      try {
        const res = await fetch(url);
        if (!res.ok) throw new Error(`Failed to fetch visualizer: ${res.statusText}`);
        const html = await res.text();
        const path = storage.getVisualizerPath();
        await writeFile(path, html);
        return c.json({ success: true });
      } catch (e) {
        return c.json({ error: String(e) }, 500);
      }
    });

    // Run pahcer
    api.post('/run', async (c) => {
      const body = await c.req.json();
      const args = body.args || [];
      
      return streamText(c, async (stream) => {
        const child = spawn('pahcer', ['run', ...args], {
          stdio: ['ignore', 'pipe', 'pipe'],
          shell: true
        });

        const jobId = Date.now().toString();
        const fullLogs: string[] = [];
        
        await storage.saveJob({
          id: jobId,
          datetime: new Date().toISOString(),
          command: 'run',
          args,
          status: 'running'
        });

        child.stdout.on('data', (data) => {
          const text = data.toString();
          fullLogs.push(text);
          stream.write(JSON.stringify({ type: 'stdout', data: text }) + '\n');
        });

        child.stderr.on('data', (data) => {
          const text = data.toString();
          fullLogs.push(text);
          stream.write(JSON.stringify({ type: 'stderr', data: text }) + '\n');
        });

        const exitCode = await new Promise<number>((resolve) => {
          child.on('close', (code) => resolve(code ?? 0));
        });

        // Simple extraction logic (can be improved based on actual pahcer output)
        const allOutput = fullLogs.join('');
        const scoreMatch = allOutput.match(/Score\s*=\s*([\d,]+)/i);
        const score = scoreMatch ? parseInt(scoreMatch[1].replace(/,/g, '')) : undefined;

        await storage.saveJob({
          id: jobId,
          datetime: new Date().toISOString(),
          command: 'run',
          args,
          status: exitCode === 0 ? 'success' : 'failed',
          result: {
            score,
            logs: allOutput
          }
        });

        stream.write(JSON.stringify({ type: 'exit', code: exitCode }) + '\n');
      });
    });

    // List results (wrapper for pahcer list)
    api.get('/list', async (c) => {
      try {
        const output = execSync('pahcer list', { encoding: 'utf-8' });
        const lines = output.trim().split('\n');
        
        // Very basic parsing: first line header, others data
        // Assume space separated for now
        const header = lines[0]?.split(/\s+/).filter(Boolean) || [];
        const rows = lines.slice(1).map(line => {
            const cols = line.split(/\s+/).filter(Boolean);
            const obj: Record<string, string> = {};
            header.forEach((h, i) => {
                obj[h] = cols[i] || '';
            });
            return obj;
        });

        return c.json({ raw: output, parsed: rows });
      } catch (e) {
        return c.json({ error: 'Failed to run pahcer list' }, 500);
      }
    });

    app.route('/api', api);

    // Serve Visualizer HTML specially if needed
    app.get('/visualizer.html', async (c) => {
        const path = storage.getVisualizerPath();
        try {
            const content = await readFile(path);
            return c.html(content.toString());
        } catch {
            return c.text('Visualizer not found. Please download it from settings.', 404);
        }
    });

    // Static files
    const relativeDistDir = relative(process.cwd(), distDir);
    app.use('/*', serveStatic({ root: relativeDistDir }));
    app.get('*', serveStatic({ root: relativeDistDir, path: 'index.html' }));

    const port = parseInt(options.port);
    console.log(`Starting server on http://localhost:${port}`);
    
    try {
      serve({
        fetch: app.fetch,
        port
      }, (info) => {
        console.log(`Server is actually listening on http://localhost:${info.port}`);
      });
    } catch (err) {
      console.error('Failed to start server:', err);
    }
} 