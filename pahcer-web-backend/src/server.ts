import { fileURLToPath } from 'node:url';
import { Hono } from 'hono';
import { serve } from '@hono/node-server';
import { serveStatic } from '@hono/node-server/serve-static';
import { dirname, join, resolve, relative } from 'node:path';
import { spawn, execSync } from 'node:child_process';

import { Storage, type GlobalConfig, type LocalConfig } from './storage.js';
import { streamText } from 'hono/streaming';
import { writeFile, readFile, mkdir, rm, readdir, stat, copyFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';

function getPahcerListParsed(baseDir: string) {
    try {
        const output = execSync('pahcer list', { encoding: 'utf-8', cwd: baseDir });
        const lines = output.trim().split('\n');
        
        // Very basic parsing: first line header, others data
        const header = lines[0]?.split(/\s+/).filter(Boolean) || [];
        const rows = lines.slice(1).map(line => {
            const cols = line.split(/\s+/).filter(Boolean);
            const obj: Record<string, string> = {};
            header.forEach((h, i) => {
                obj[h] = cols[i] || '';
            });
            return obj;
        });
        return { raw: output, parsed: rows };
    } catch (e) {
        return { raw: '', parsed: [] };
    }
}

async function getSeeds(baseDir: string): Promise<string[]> {
    const seedsPath = join(baseDir, 'tools', 'seeds.txt');
    const inDir = join(baseDir, 'tools', 'in');
    
    try {
        if (existsSync(seedsPath)) {
            const content = await readFile(seedsPath, 'utf-8');
            return content.trim().split('\n').map(s => s.trim()).filter(Boolean);
        } else if (existsSync(inDir)) {
            const files = await readdir(inDir);
            const numericFiles = files
                .filter(f => f.endsWith('.txt') && !isNaN(Number(f.replace('.txt', ''))))
                .sort((a, b) => Number(a.replace('.txt', '')) - Number(b.replace('.txt', '')));
            return numericFiles.map(f => Number(f.replace('.txt', '')).toString());
        }
    } catch {}
    return [];
}

async function downloadRecursive(url: string, destDir: string) {
    const visited = new Set<string>();
    const queue: { url: string; relPath: string }[] = [{ url, relPath: 'index.html' }];

    // 同名の .js と _bg.wasm を明示的に追加（wasm-bindgen等で動的にロードされる場合があるため）
    try {
        const u = new URL(url);
        const path = u.pathname;
        if (path.endsWith('.html')) {
            const base = path.substring(path.lastIndexOf('/') + 1, path.lastIndexOf('.html'));
            const dirUrl = url.substring(0, url.lastIndexOf('/') + 1);
            queue.push({ url: dirUrl + base + '.js', relPath: base + '.js' });
            queue.push({ url: dirUrl + base + '_bg.wasm', relPath: base + '_bg.wasm' });
        }
    } catch (e) {
        console.error('Failed to parse base name for visualizer siblings:', e);
    }

    await mkdir(destDir, { recursive: true });

    while (queue.length > 0) {
        const { url: currentUrl, relPath } = queue.shift()!;
        if (visited.has(currentUrl)) continue;
        visited.add(currentUrl);

        try {
            const res = await fetch(currentUrl);
            if (!res.ok) continue;

            const contentType = res.headers.get('content-type') || '';
            const buffer = await res.arrayBuffer();
            const filePath = join(destDir, relPath);
            await mkdir(dirname(filePath), { recursive: true });
            await writeFile(filePath, Buffer.from(buffer));

            // HTMLまたはCSSをパースして追加のリソースを探す
            if (contentType.includes('text/html') || contentType.includes('text/css')) {
                const text = new TextDecoder().decode(buffer);
                // HTML用
                const resourceRegex = /(?:src|href|content)\s*=\s*["']([^"']+\.(?:js|css|wasm|png|jpg|svg|ico|json))["']/gi;
                // CSS用 (url(...))
                const cssResourceRegex = /url\(['"]?([^'"]+\.(?:png|jpg|svg|ico|wasm|woff2?))['"]?\)/gi;
                
                let match;
                while ((match = resourceRegex.exec(text)) !== null) {
                    const foundPath = match[1];
                    if (foundPath.startsWith('http') || foundPath.startsWith('//') || foundPath.startsWith('data:')) continue;
                    
                    const resourceUrl = new URL(foundPath, currentUrl).toString();
                    if (!visited.has(resourceUrl)) {
                        queue.push({ url: resourceUrl, relPath: foundPath.replace(/^\//, '') });
                    }
                }
                while ((match = cssResourceRegex.exec(text)) !== null) {
                    const foundPath = match[1];
                    if (foundPath.startsWith('http') || foundPath.startsWith('//') || foundPath.startsWith('data:')) continue;
                    
                    const resourceUrl = new URL(foundPath, currentUrl).toString();
                    if (!visited.has(resourceUrl)) {
                        queue.push({ url: resourceUrl, relPath: foundPath.replace(/^\//, '') });
                    }
                }
            }
        } catch (e) {
            console.error(`Failed to download ${currentUrl}:`, e);
        }
    }
}

export async function startServer(options: any) {
    const baseDir = resolve(options.directory || process.cwd());
    const storage = new Storage(baseDir);

    // Read problem_name from pahcer_config.toml
    let problemName = 'unknown';
    try {
        const configPath = join(baseDir, 'pahcer_config.toml');
        if (existsSync(configPath)) {
            const content = await readFile(configPath, 'utf-8');
            const match = content.match(/problem_name\s*=\s*["']([^"']+)["']/);
            if (match) {
                problemName = match[1];
            }
        }
    } catch (e) {
        console.error('Failed to read pahcer_config.toml:', e);
    }

    const __filename = fileURLToPath(import.meta.url);
    const __dirname = dirname(__filename);
    const backendRoot = resolve(__dirname, '..'); 
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
      return c.json({ global, local, problemName });
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

    // History
    api.get('/history', async (c) => {
        const resultsDir = storage.getResultsDir();
        try {
            const dirs = await readdir(resultsDir);
            const results = [];
            for (const dir of dirs) {
                try {
                    const resultPath = join(resultsDir, dir, 'result.json');
                    const content = await readFile(resultPath, 'utf-8');
                    results.push(JSON.parse(content));
                } catch (e) {
                    // ignore invalid/incomplete results
                }
            }
            // Sort by datetime desc
            results.sort((a, b) => new Date(b.datetime).getTime() - new Date(a.datetime).getTime());
            return c.json(results);
        } catch (e) {
            return c.json([]);
        }
    });

    api.get('/history/:timestamp/output/:filename', async (c) => {
        const timestamp = c.req.param('timestamp');
        const filename = c.req.param('filename');
        const filePath = join(storage.getResultsDir(), timestamp, 'output', filename);
        try {
            const content = await readFile(filePath);
            return c.text(content.toString());
        } catch {
            return c.text('Not Found', 404);
        }
    });

    // Visualizer
    api.get('/visualizer/status', async (c) => {
        const exists = await storage.hasVisualizer();
        return c.json({ exists });
    });

    api.post('/visualizer/download', async (c) => {
      const { url } = await c.req.json();
      if (!url) return c.json({ error: 'URL is required' }, 400);
      
      try {
        const destDir = storage.getVisualizerDir();
        // Clear existing
        try { await rm(destDir, { recursive: true, force: true }); } catch {}
        
        await downloadRecursive(url, destDir);
        return c.json({ success: true });
      } catch (e) {
        return c.json({ error: String(e) }, 500);
      }
    });

    // Analysis
    api.post('/analysis/download', async (c) => {
      const url = 'https://img.atcoder.jp/ahc_standings/index.html';
      try {
        const res = await fetch(url);
        if (!res.ok) throw new Error(`Failed to fetch analysis tool: ${res.statusText}`);
        const html = await res.text();
        const path = storage.getAnalysisPath();
        await writeFile(path, html);
        return c.json({ success: true });
      } catch (e) {
        return c.json({ error: String(e) }, 500);
      }
    });

    // Run pahcer
    api.post('/run', async (c) => {
      const body = await c.req.json();
      let args: string[] = body.args || [];
      
      // Force --json
      if (!args.includes('--json') && !args.includes('-j')) {
        args.push('--json');
      }

      // Extract metadata
      let comment = '';
      let tag = '';
      for (let i = 0; i < args.length; i++) {
          if (args[i] === '-c' || args[i] === '--comment') {
              comment = args[i+1] || '';
              // Quote the comment for shell execution if it isn't quoted
              if (args[i+1] && !args[i+1].startsWith('"') && !args[i+1].startsWith("'")) {
                  args[i+1] = `"${args[i+1]}"`;
              }
          }
          if (args[i] === '-t' || args[i] === '--tag') {
              tag = args[i+1] || '';
              // Quote the tag for shell execution if it isn't quoted
              if (args[i+1] && !args[i+1].startsWith('"') && !args[i+1].startsWith("'")) {
                  args[i+1] = `"${args[i+1]}"`;
              }
          }
      }

      // Create timestamped directory
      const timestamp = Math.floor(Date.now() / 1000).toString();
      const resultDir = join(storage.getResultsDir(), timestamp);
      const outputDir = join(resultDir, 'output');
      await mkdir(outputDir, { recursive: true });

      return streamText(c, async (stream) => {
        const child = spawn('pahcer', ['run', ...args], {
          stdio: ['ignore', 'pipe', 'pipe'],
          shell: true,
          cwd: baseDir
        });

        const jobId = Date.now().toString();
        const fullLogs: string[] = [];
        
        // Keep jobs.json for now as a running indicator/log storage if needed, 
        // but result.json is the main storage for history.
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

        const allOutput = fullLogs.join('');
        
        // Parse JSON output from pahcer (NDJSON support)
        let collectedDetails: any[] = [];
        const lines = allOutput.split('\n');
        
        for (const line of lines) {
            const trimmed = line.trim();
            if (!trimmed) continue;
            try {
                const obj = JSON.parse(trimmed);
                // Check for single case result
                if (obj.seed !== undefined && obj.score !== undefined) {
                    collectedDetails.push(obj);
                } 
                // Check for array of results (summary)
                else if (Array.isArray(obj)) {
                    collectedDetails = collectedDetails.concat(obj);
                }
                // Check for object with results
                else if (obj.results && Array.isArray(obj.results)) {
                    collectedDetails = collectedDetails.concat(obj.results);
                }
            } catch (e) {
                // Ignore non-JSON lines
            }
        }

        // Deduplicate by seed
        const detailsMap = new Map<string, any>();
        for (const d of collectedDetails) {
            if (d.seed !== undefined) {
                detailsMap.set(String(d.seed), d);
            }
        }
        
        const details = Array.from(detailsMap.values());

        // Default stats
        let stats = {
            avgScore: 0,
            avgLogScore: 0,
            avgRelativeScore: 0,
            maxTime: 0,
            cases: details.length,
            details: details
        };

        if (stats.cases > 0) {
            const totalScore = details.reduce((sum, r) => sum + (Number(r.score) || 0), 0);
            stats.avgScore = totalScore / stats.cases;
            
            const totalLogScore = details.reduce((sum, r) => sum + Math.log10(Math.max(1, Number(r.score) || 0)), 0);
            stats.avgLogScore = totalLogScore / stats.cases;

            const totalRelativeScore = details.reduce((sum, r) => sum + (Number(r.relative_score) || 0), 0);
            stats.avgRelativeScore = totalRelativeScore / stats.cases;

            stats.maxTime = Math.max(...details.map(r => {
                // Use execution_time (seconds) * 1000 => ms, or time (unknown unit, assume seconds if small?)
                // Pahcer output 'execution_time' is seconds.
                const t = r.execution_time !== undefined ? Number(r.execution_time) : (Number(r.time) || 0);
                return t * 1000;
            }));
        } else {
             // Fallback to regex extraction if JSON parsing failed completely
             const scoreMatch = allOutput.match(/Score\s*=\s*([\d,]+)/i);
             const score = scoreMatch ? parseInt(scoreMatch[1].replace(/,/g, '')) : 0;
             if (score > 0) {
                 stats.avgScore = score;
                 stats.avgLogScore = Math.log10(score);
                 stats.cases = 1; 
             }
        }

        if (stats.cases > 0) {
            // Copy output files
            // Assume tools/out/*.txt
            const toolsOutDir = join(baseDir, 'tools', 'out');
            try {
                if (existsSync(toolsOutDir)) {
                    const files = await readdir(toolsOutDir);
                    for (const file of files) {
                        if (file.endsWith('.txt')) {
                            await copyFile(join(toolsOutDir, file), join(outputDir, file));
                        }
                    }
                }
            } catch (e) {
                console.error('Failed to copy output files', e);
            }

            // Save result.json
            const resultJson = {
                id: timestamp,
                datetime: new Date().toISOString(),
                args,
                comment,
                tag,
                ...stats
            };
            await writeFile(join(resultDir, 'result.json'), JSON.stringify(resultJson, null, 2));

            await storage.saveJob({
              id: jobId,
              datetime: new Date().toISOString(),
              command: 'run',
              args,
              status: exitCode === 0 ? 'success' : 'failed',
              result: {
                score: stats.avgScore,
                logs: allOutput
              }
            });
        } else {
            // Cleanup empty result directory if no cases were executed
            try {
                await rm(resultDir, { recursive: true, force: true });
            } catch (e) {
                console.error('Failed to cleanup empty result dir', e);
            }
            // Remove the job entry as well
            await storage.deleteJob(jobId);
        }

        stream.write(JSON.stringify({ type: 'exit', code: exitCode }) + '\n');
      });
    });

    // List results (wrapper for pahcer list)
    api.get('/list', async (c) => {
      const result = getPahcerListParsed(baseDir);
      return c.json(result);
    });

    app.route('/api', api);

    // Serve Visualizer Directory
    const visualizerDir = storage.getVisualizerDir();
    app.get('/visualizer/*', async (c) => {
        const relPath = c.req.path.replace('/visualizer/', '') || 'index.html';
        const filePath = join(visualizerDir, relPath);
        
        // Helper to inject script
        const injectScript = (html: string) => {
            const script = `
            <script>
            (async function() {
                const scrollKey = 'pahcer_v_scroll';
                const restoreScroll = () => {
                    const saved = sessionStorage.getItem(scrollKey);
                    if (saved) {
                        const { x, y } = JSON.parse(saved);
                        window.scrollTo(x, y);
                    }
                };

                // Save scroll position before unload or periodically
                window.addEventListener('scroll', () => {
                    sessionStorage.setItem(scrollKey, JSON.stringify({ x: window.pageXOffset, y: window.pageYOffset }));
                }, { passive: true });

                try {
                    const params = new URLSearchParams(window.location.search);
                    const outputUrl = params.get('output_url');
                    if (outputUrl) {
                        const res = await fetch(outputUrl);
                        if (res.ok) {
                            const text = await res.text();
                            const el = document.getElementById('output') || 
                                       document.getElementById('input') || 
                                       document.querySelector('textarea');
                            if (el) {
                                el.value = text;
                                el.dispatchEvent(new Event('input', { bubbles: true }));
                                el.dispatchEvent(new Event('change', { bubbles: true }));
                                // Restore scroll after potential layout changes from input
                                setTimeout(restoreScroll, 10);
                            }
                        }
                    }
                } catch(e) { console.error('Failed to inject output:', e); }
                
                // Initial restore
                restoreScroll();
            })();
            </script>
            `;
            return html.replace('</body>', `${script}</body>`);
        };

        try {
            const content = await readFile(filePath);
            
            if (relPath.endsWith('.html')) {
                const html = content.toString();
                return c.html(injectScript(html));
            }

            const contentType = relPath.endsWith('.js') ? 'application/javascript' : 
                               relPath.endsWith('.css') ? 'text/css' : 
                               relPath.endsWith('.wasm') ? 'application/wasm' :
                               relPath.endsWith('.png') ? 'image/png' :
                               relPath.endsWith('.jpg') ? 'image/jpeg' :
                               relPath.endsWith('.svg') ? 'image/svg+xml' :
                               'application/octet-stream';
            return c.body(content, 200, { 'Content-Type': contentType });
        } catch {
            return c.text('Not Found', 404);
        }
    });

    app.get('/visualizer.html', async (c) => {
        const path = storage.getVisualizerPath();
        try {
            const content = await readFile(path);
            let html = content.toString();
            
             const script = `
            <script>
            (async function() {
                const scrollKey = 'pahcer_v_scroll';
                const restoreScroll = () => {
                    const saved = sessionStorage.getItem(scrollKey);
                    if (saved) {
                        const { x, y } = JSON.parse(saved);
                        window.scrollTo(x, y);
                    }
                };

                window.addEventListener('scroll', () => {
                    sessionStorage.setItem(scrollKey, JSON.stringify({ x: window.pageXOffset, y: window.pageYOffset }));
                }, { passive: true });

                try {
                    const params = new URLSearchParams(window.location.search);
                    const outputUrl = params.get('output_url');
                    if (outputUrl) {
                        const res = await fetch(outputUrl);
                        if (res.ok) {
                            const text = await res.text();
                            const el = document.getElementById('output') || 
                                       document.getElementById('input') || 
                                       document.querySelector('textarea');
                            if (el) {
                                el.value = text;
                                el.dispatchEvent(new Event('input', { bubbles: true }));
                                el.dispatchEvent(new Event('change', { bubbles: true }));
                                setTimeout(restoreScroll, 10);
                            }
                        }
                    }
                } catch(e) { console.error('Failed to inject output:', e); }
                
                restoreScroll();
            })();
            </script>
            `;
            html = html.replace('</body>', `${script}</body>`);
            
            return c.html(html);
        } catch {
            return c.text('Visualizer not found. Please download it from settings.', 404);
        }
    });

    // Serve Analysis Tool
    app.get('/analysis/index.html', async (c) => {
        const path = storage.getAnalysisPath();
        try {
            const content = await readFile(path);
            return c.html(content.toString());
        } catch {
             // Try to download if missing
             try {
                const url = 'https://img.atcoder.jp/ahc_standings/index.html';
                const res = await fetch(url);
                if (res.ok) {
                    const html = await res.text();
                    await writeFile(path, html);
                    return c.html(html);
                }
             } catch {}
             return c.text('Analysis tool not found and failed to download automatically.', 404);
        }
    });

    app.get('/analysis/:contest/input.csv', async (c) => {
        const seeds = await getSeeds(baseDir);
        const inDir = join(baseDir, 'tools', 'in');

        if (seeds.length === 0) {
            return c.text('file,seed\n');
        }

        // Determine columns from first file
        let paramNames: string[] = [];
        try {
            // Try 0000.txt first
            const firstFile = '0000.txt'; 
            const filePath = join(inDir, firstFile);
            if (existsSync(filePath)) {
                const content = await readFile(filePath, 'utf-8');
                const firstLine = content.split('\n')[0].trim();
                const params = firstLine.split(/\s+/);
                const defaultNames = ['N', 'M', 'L', 'K', 'T', 'S'];
                paramNames = params.map((_, i) => defaultNames[i] || `p_${i}`);
            }
        } catch {}

        let csv = `file,seed,${paramNames.join(',')}\n`;

        for (let i = 0; i < seeds.length; i++) {
            const seed = seeds[i];
            const filename = String(i).padStart(4, '0') + '.txt';
            const filePath = join(inDir, filename);
            
            let paramsPart = '';
            try {
                if (existsSync(filePath)) {
                    const content = await readFile(filePath, 'utf-8');
                    const firstLine = content.split('\n')[0].trim();
                    paramsPart = firstLine.split(/\s+/).join(',');
                }
            } catch {}

            if (!paramsPart && paramNames.length > 0) {
                paramsPart = new Array(paramNames.length).fill('').join(',');
            }

            csv += `${filename},${seed}${paramsPart ? ',' + paramsPart : ''}\n`;
        }

        return c.text(csv);
    });

    app.get('/analysis/:contest/result.csv', async (c) => {
        const seeds = await getSeeds(baseDir);
        
        // 2. Header
        const config = await storage.getGlobalConfig();
        const localConfig = await storage.getLocalConfig();
        const visualizerUrl = localConfig.visualizerUrl || config.visualizerUrl || '';
        
        let csv = `raw,1000000000,${visualizerUrl}\n`;

        // 3. Rows
        const resultsDir = storage.getResultsDir();
        try {
            const dirs = await readdir(resultsDir);
            for (const dir of dirs) {
                try {
                    const resultPath = join(resultsDir, dir, 'result.json');
                    const content = await readFile(resultPath, 'utf-8');
                    const result = JSON.parse(content);
                    
                    let author = result.tag || result.comment;
                    if (!author) {
                        const date = new Date(result.datetime);
                        author = date.toLocaleString(); 
                    }
                    // Sanitize
                    author = author.replace(/,/g, ' ').replace(/"/g, '').trim();

                    // Create score map
                    const scoreMap = new Map<string, number>();
                    if (result.details && Array.isArray(result.details)) {
                        for (const d of result.details) {
                            if (d.seed !== undefined) {
                                scoreMap.set(String(d.seed), Number(d.score));
                            }
                        }
                    }

                    // Generate row
                    const scores = seeds.map(s => scoreMap.has(s) ? scoreMap.get(s) : -1);
                    csv += `${author},${scores.join(',')}\n`;
                } catch {}
            }
        } catch {}
        return c.text(csv);
    });

    app.get('/analysis/*', (c) => c.text('Not Found', 404));

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
