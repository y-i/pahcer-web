export interface TestRunOptions {
  shuffle: boolean;
  comment: string;
  tag: string;
  settingFile: string;
  freezeBestScores: boolean;
  noCompile: boolean;
}

export interface TestRunNotificationSettings {
  testRunCompleted: boolean;
}

export type InitializationState = 'initialized' | 'uninitialized' | 'invalid';
export type InitObjective = 'max' | 'min';
export type InitLanguage = 'cpp' | 'python' | 'rust' | 'go';

export type PersistedTestRunOptions = Omit<TestRunOptions, 'comment' | 'tag'>;

export interface RunLogEntry {
  type: 'stdout' | 'stderr' | 'info';
  text: string;
}

export type RunStreamEvent =
  | { type: 'stdout'; data: string }
  | { type: 'stderr'; data: string }
  | { type: 'exit'; code: number };

export const DEFAULT_TEST_RUN_OPTIONS: TestRunOptions = {
  shuffle: false,
  comment: '',
  tag: '',
  settingFile: 'pahcer_config.toml',
  freezeBestScores: false,
  noCompile: false,
};

export const DEFAULT_NOTIFICATION_SETTINGS: TestRunNotificationSettings = {
  testRunCompleted: false,
};

export type ResultJsonMode = 'symlink' | 'copy';
export type VisualizerInitialScrollPosition = 'top' | 'bottom';
export type HistoryScoreDisplayFormat = 'plain' | 'scientific';

export interface GlobalConfig {
  visualizerPosition: 'left' | 'right';
  visualizerInitialScrollPosition: VisualizerInitialScrollPosition;
  visualizerUrl?: string;
  resultJsonMode: ResultJsonMode;
  defaultSeed: number;
  defaultScale: number;
  testRunOptions?: PersistedTestRunOptions;
  notifications?: TestRunNotificationSettings;
}

export interface LocalConfig {
  visualizerUrl?: string;
  defaultScoreType?: 'raw' | 'max' | 'min' | 'rank_max' | 'rank_min';
  historyScoreDisplayFormat?: HistoryScoreDisplayFormat;
  inputParamNames?: string;
}

export interface ConfigResponse {
  initializationState: InitializationState;
  initializationError: string | null;
  global: GlobalConfig;
  local: LocalConfig;
  problemName: string | null;
  baseDir: string;
}

export interface InitRequest {
  problem: string;
  objective: InitObjective;
  language: InitLanguage;
  interactive: boolean;
}

export interface JobMetadata {
  id: string;
  datetime: string;
  command: string;
  args: string[];
  status: 'running' | 'success' | 'failed';
}

export async function readApiError(response: Response, fallbackMessage: string): Promise<Error> {
  const contentType = response.headers.get('content-type') ?? '';

  if (contentType.includes('application/json')) {
    const payload = await response.json().catch(() => null) as { error?: string } | null;
    if (payload?.error) {
      return new Error(payload.error);
    }
  }

  const text = await response.text().catch(() => '');
  return new Error(text || fallbackMessage);
}

export async function assertOk(response: Response, fallbackMessage: string): Promise<void> {
  if (!response.ok) {
    throw await readApiError(response, fallbackMessage);
  }
}

export const api = {
  async getConfig(): Promise<ConfigResponse> {
    const res = await fetch('/api/config');
    await assertOk(res, 'Failed to load config');
    return res.json();
  },

  async initProject(request: InitRequest): Promise<ConfigResponse> {
    const res = await fetch('/api/init', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(request),
    });
    await assertOk(res, 'Failed to initialize project');
    return res.json();
  },

  async saveGlobalConfig(config: GlobalConfig): Promise<void> {
    const res = await fetch('/api/config/global', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(config),
    });
    await assertOk(res, 'Failed to save global config');
  },

  async saveLocalConfig(config: LocalConfig): Promise<void> {
    const res = await fetch('/api/config/local', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(config),
    });
    await assertOk(res, 'Failed to save local config');
  },

  async getJobs(): Promise<JobMetadata[]> {
    const res = await fetch('/api/jobs');
    await assertOk(res, 'Failed to load jobs');
    return res.json();
  },

  async getVisualizerStatus(): Promise<{ exists: boolean }> {
    const res = await fetch('/api/visualizer/status');
    await assertOk(res, 'Failed to check visualizer status');
    return res.json();
  },

  async downloadVisualizer(url: string): Promise<void> {
    const res = await fetch('/api/visualizer/download', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ url }),
    });
    await assertOk(res, 'Failed to download visualizer');
  },

  async getList(): Promise<{ raw: string; parsed: Record<string, string>[] }> {
    const res = await fetch('/api/list');
    await assertOk(res, 'Failed to load pahcer list');
    return res.json();
  },

  async getHistory(): Promise<any[]> {
    const res = await fetch('/api/history');
    await assertOk(res, 'Failed to load history');
    return res.json();
  },

  async deleteHistory(timestamp: string): Promise<void> {
    const res = await fetch(`/api/history/${timestamp}`, {
      method: 'DELETE',
    });
    await assertOk(res, 'Failed to delete history');
  },

  async runPahcer(args: string[], onData: (data: RunStreamEvent) => void): Promise<void> {
    const res = await fetch('/api/run', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ args }),
    });

    await assertOk(res, 'Failed to start pahcer run');

    const reader = res.body?.getReader();
    if (!reader) return;

    const decoder = new TextDecoder();
    let buffer = '';

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split('\n');
      buffer = lines.pop() || '';

      for (const line of lines) {
        if (line.trim()) {
          try {
            const data = JSON.parse(line) as RunStreamEvent;
            onData(data);
          } catch (e) {
            console.error('Failed to parse SSE line:', line);
          }
        }
      }
    }
  }
};
