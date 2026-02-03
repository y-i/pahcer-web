export interface GlobalConfig {
  visualizerPosition: 'left' | 'right';
  visualizerUrl?: string;
  defaultSeed: number;
  defaultScale: number;
  testRunOptions?: {
    shuffle: boolean;
    json: boolean;
    settingFile: string;
    freezeBestScores: boolean;
    noResultFile: boolean;
    noCompile: boolean;
    extraArgs: string;
  };
}

export interface LocalConfig {
  visualizerUrl?: string;
}

export interface ConfigResponse {
  global: GlobalConfig;
  local: LocalConfig;
}

export interface JobMetadata {
  id: string;
  datetime: string;
  command: string;
  args: string[];
  status: 'running' | 'success' | 'failed';
}

export const api = {
  async getConfig(): Promise<ConfigResponse> {
    const res = await fetch('/api/config');
    return res.json();
  },

  async saveGlobalConfig(config: GlobalConfig): Promise<void> {
    await fetch('/api/config/global', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(config),
    });
  },

  async saveLocalConfig(config: LocalConfig): Promise<void> {
    await fetch('/api/config/local', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(config),
    });
  },

  async getJobs(): Promise<JobMetadata[]> {
    const res = await fetch('/api/jobs');
    return res.json();
  },

  async downloadVisualizer(url: string): Promise<void> {
    const res = await fetch('/api/visualizer/download', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ url }),
    });
    if (!res.ok) {
        const err = await res.json();
        throw new Error(err.error || 'Failed to download visualizer');
    }
  },

  async getList(): Promise<{ raw: string; parsed: Record<string, string>[] }> {
    const res = await fetch('/api/list');
    return res.json();
  },

  async runPahcer(args: string[], onData: (data: any) => void): Promise<void> {
    const res = await fetch('/api/run', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ args }),
    });

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
            const data = JSON.parse(line);
            onData(data);
          } catch (e) {
            console.error('Failed to parse SSE line:', line);
          }
        }
      }
    }
  }
};
