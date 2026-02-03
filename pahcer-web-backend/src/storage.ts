import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { homedir } from 'node:os';
import { z } from 'zod';

// --- Schemas ---

export const GlobalConfigSchema = z.object({
  visualizerPosition: z.enum(['left', 'right']).default('right'),
  visualizerUrl: z.string().url().optional(),
  defaultSeed: z.number().default(0),
  defaultScale: z.number().default(1.0),
  testRunOptions: z.object({
    shuffle: z.boolean().default(false),
    json: z.boolean().default(false),
    settingFile: z.string().default('pahcer_config.toml'),
    freezeBestScores: z.boolean().default(false),
    noResultFile: z.boolean().default(false),
    noCompile: z.boolean().default(false),
    extraArgs: z.string().default(''),
  }).optional(),
});

export type GlobalConfig = z.infer<typeof GlobalConfigSchema>;

export const LocalConfigSchema = z.object({
  visualizerUrl: z.string().url().optional(),
});

export type LocalConfig = z.infer<typeof LocalConfigSchema>;

export const JobMetadataSchema = z.object({
  id: z.string(),
  datetime: z.string(),
  command: z.string(),
  args: z.array(z.string()),
  status: z.enum(['running', 'success', 'failed']),
  outputFile: z.string().optional(),
  result: z.record(z.any()).optional(), // Store detailed results (score, etc.)
});

export type JobMetadata = z.infer<typeof JobMetadataSchema>;

export const JobsSchema = z.array(JobMetadataSchema);

// --- Storage Implementation ---

export class Storage {
  private globalConfigPath: string;
  private localDir: string = '.pahcer-web';

  constructor() {
    const configHome = process.env.XDG_CONFIG_HOME || join(homedir(), '.config');
    this.globalConfigPath = join(configHome, 'pahcer-web', 'config.json');
  }

  // Global Config
  async getGlobalConfig(): Promise<GlobalConfig> {
    try {
      const data = await readFile(this.globalConfigPath, 'utf-8');
      return GlobalConfigSchema.parse(JSON.parse(data));
    } catch {
      return GlobalConfigSchema.parse({});
    }
  }

  async saveGlobalConfig(config: GlobalConfig): Promise<void> {
    await mkdir(join(this.globalConfigPath, '..'), { recursive: true });
    await writeFile(this.globalConfigPath, JSON.stringify(config, null, 2));
  }

  // Local Config
  private getLocalConfigPath(): string {
    return join(process.cwd(), this.localDir, 'config.json');
  }

  async getLocalConfig(): Promise<LocalConfig> {
    try {
      const data = await readFile(this.getLocalConfigPath(), 'utf-8');
      return LocalConfigSchema.parse(JSON.parse(data));
    } catch {
      return LocalConfigSchema.parse({});
    }
  }

  async saveLocalConfig(config: LocalConfig): Promise<void> {
    await mkdir(join(process.cwd(), this.localDir), { recursive: true });
    await writeFile(this.getLocalConfigPath(), JSON.stringify(config, null, 2));
  }

  // Jobs
  private getJobsPath(): string {
    return join(process.cwd(), this.localDir, 'jobs.json');
  }

  async getJobs(): Promise<JobMetadata[]> {
    try {
      const data = await readFile(this.getJobsPath(), 'utf-8');
      return JobsSchema.parse(JSON.parse(data));
    } catch {
      return [];
    }
  }

  async saveJob(job: JobMetadata): Promise<void> {
    const jobs = await this.getJobs();
    const index = jobs.findIndex(j => j.id === job.id);
    if (index >= 0) {
      jobs[index] = job;
    } else {
      jobs.push(job);
    }
    await mkdir(join(process.cwd(), this.localDir), { recursive: true });
    await writeFile(this.getJobsPath(), JSON.stringify(jobs, null, 2));
  }

  // Visualizer
  getVisualizerPath(): string {
    return join(process.cwd(), this.localDir, 'visualizer.html');
  }
}
