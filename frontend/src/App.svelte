<script lang="ts">
  import { onMount } from 'svelte';
  import Navbar from './lib/Navbar.svelte';
  import TestRun from './lib/TestRun.svelte';
  import History from './lib/History.svelte';
  import ScoreAnalysis from './lib/ScoreAnalysis.svelte';
  import Settings from './lib/Settings.svelte';
  import InitScreen from './lib/InitScreen.svelte';
  import { api, type ConfigResponse } from './lib/api';
  import { buildTabHref } from './lib/navigation';

  const tabs = [
    { id: 'test-run', label: 'テスト実行' },
    { id: 'history', label: '実行履歴' },
    { id: 'score-analysis', label: 'スコア分析' },
    { id: 'settings', label: '設定' },
  ];

  function getTabFromPath() {
    const path = window.location.pathname.slice(1);
    return tabs.find(t => t.id === path)?.id ?? 'test-run';
  }

  let activeTab = $state('test-run');
  let config = $state<ConfigResponse | null>(null);
  let isLoading = $state(true);
  let loadError = $state('');

  onMount(() => {
    activeTab = getTabFromPath();
    void loadConfig();
  });

  async function loadConfig() {
    isLoading = true;
    loadError = '';

    try {
      const nextConfig = await api.getConfig();
      config = nextConfig;
      if (nextConfig.initializationState === 'initialized') {
        activeTab = getTabFromPath();
      }
    } catch (error) {
      loadError = error instanceof Error ? error.message : 'Failed to load config';
    } finally {
      isLoading = false;
    }
  }

  function handleInitialized(nextConfig: ConfigResponse) {
    config = nextConfig;
    activeTab = 'test-run';
    history.pushState(null, '', buildTabHref('test-run', window.location));
  }

  function handleConfigChange(nextConfig: ConfigResponse) {
    config = nextConfig;
  }

  $effect(() => {
    const handlePopState = () => {
      if (config?.initializationState !== 'initialized') {
        return;
      }
      activeTab = getTabFromPath();
    };
    window.addEventListener('popstate', handlePopState);
    return () => {
      window.removeEventListener('popstate', handlePopState);
    };
  });

  $effect(() => {
    if (config?.initializationState !== 'initialized') {
      return;
    }

    const path = window.location.pathname.slice(1);
    const currentTab = path === '' ? 'test-run' : path;
    
    if (activeTab !== currentTab) {
      history.pushState(null, '', buildTabHref(activeTab, window.location));
    }
  });
</script>

<div class="h-full bg-gray-50 flex flex-col overflow-hidden">
  {#if isLoading}
    <main class="flex-1 min-h-0 flex items-center justify-center px-6">
      <div class="flex items-center gap-3 rounded-2xl border border-gray-200 bg-white px-5 py-4 text-gray-600 shadow-sm">
        <svg class="h-5 w-5 animate-spin text-indigo-600" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        <span>設定状態を確認しています...</span>
      </div>
    </main>
  {:else if loadError}
    <main class="flex-1 min-h-0 flex items-center justify-center px-6">
      <section class="w-full max-w-2xl rounded-3xl border border-red-200 bg-white p-8 shadow-lg shadow-red-100/50">
        <p class="text-sm font-semibold uppercase tracking-[0.3em] text-red-500">Load Error</p>
        <h1 class="mt-3 text-3xl font-bold text-gray-900">初期状態の取得に失敗しました</h1>
        <p class="mt-4 rounded-2xl bg-red-50 px-4 py-3 text-sm text-red-700">{loadError}</p>
        <div class="mt-6 flex justify-end">
          <button
            onclick={loadConfig}
            class="rounded-xl bg-gray-900 px-5 py-2.5 text-sm font-semibold text-white transition hover:bg-gray-700"
          >
            再試行
          </button>
        </div>
      </section>
    </main>
  {:else if config?.initializationState === 'uninitialized'}
    <InitScreen {config} onSuccess={handleInitialized} />
  {:else if config?.initializationState === 'invalid'}
    <main class="flex-1 min-h-0 flex items-center justify-center px-6 py-10">
      <section class="w-full max-w-3xl overflow-hidden rounded-[28px] border border-amber-200 bg-white shadow-xl shadow-amber-100/60">
        <div class="border-b border-amber-100 bg-[radial-gradient(circle_at_top_left,_rgba(251,191,36,0.35),_transparent_45%),linear-gradient(135deg,_#fff7ed,_#ffffff_55%)] px-8 py-7">
          <p class="text-sm font-semibold uppercase tracking-[0.3em] text-amber-600">Invalid Config</p>
          <h1 class="mt-3 text-3xl font-bold text-gray-900">pahcer_config.toml を読み取れません</h1>
          <p class="mt-3 max-w-2xl text-sm leading-6 text-gray-600">
            対象ディレクトリに設定ファイルは存在しますが、内容を正常に解釈できません。設定内容を修正するか、削除してから再初期化してください。
          </p>
        </div>
        <div class="px-8 py-7">
          <pre class="overflow-x-auto rounded-2xl bg-gray-950 px-4 py-4 text-sm leading-6 text-amber-100">{config.initializationError ?? 'Unknown error'}</pre>
          <div class="mt-6 flex justify-end">
            <button
              onclick={loadConfig}
              class="rounded-xl bg-gray-900 px-5 py-2.5 text-sm font-semibold text-white transition hover:bg-gray-700"
            >
              再読み込み
            </button>
          </div>
        </div>
      </section>
    </main>
  {:else if config}
    <Navbar {tabs} bind:activeTab />

    <main class="flex-1 min-h-0 overflow-hidden flex flex-col">
      {#if activeTab === 'test-run'}
        <TestRun initialConfig={config} />
      {:else if activeTab === 'history'}
        <History initialConfig={config} />
      {:else if activeTab === 'score-analysis'}
        <ScoreAnalysis initialConfig={config} />
      {:else if activeTab === 'settings'}
        <Settings initialConfig={config} onConfigChange={handleConfigChange} />
      {/if}
    </main>
  {:else}
    <main class="flex-1"></main>
  {/if}
</div>

<style>
  :global(body) {
    margin: 0;
    padding: 0;
  }
</style>