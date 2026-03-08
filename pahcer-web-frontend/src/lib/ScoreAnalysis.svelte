<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from './api';

  let iframeSrc = $state('/analysis/index.html');
  let isLoading = $state(false);
  let isCheckingConfig = $state(true);
  let hasVisualizerUrl = $state(false);
  let problemName = $state('unknown');

  onMount(async () => {
    try {
      const config = await api.getConfig();
      hasVisualizerUrl = !!(config.local.visualizerUrl || config.global.visualizerUrl);
      if (config.problemName) {
          problemName = config.problemName;
          iframeSrc = `/analysis/index.html?contest=${problemName}`;
      }
    } catch (e) {
      console.error('Failed to load config:', e);
    } finally {
      isCheckingConfig = false;
    }
  });

  async function reloadAnalysis() {
    isLoading = true;
    try {
        await fetch('/api/analysis/download', {
            method: 'POST'
        });
        iframeSrc = `/analysis/index.html?contest=${problemName}&t=${Date.now()}`;
    } catch (e) {
        console.error(e);
        alert('Failed to reload analysis tool.');
    } finally {
        isLoading = false;
    }
  }

  function goToSettings() {
    history.pushState(null, '', '/settings');
    window.dispatchEvent(new PopStateEvent('popstate'));
  }
</script>

<div class="h-full flex flex-col bg-gray-50 overflow-hidden">
  {#if isCheckingConfig}
    <div class="flex-1 flex items-center justify-center text-gray-500">
        <svg class="animate-spin h-5 w-5 mr-3 text-indigo-600" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        <span>Checking configuration...</span>
    </div>
  {:else if !hasVisualizerUrl}
    <div class="flex-1 flex overflow-hidden p-6 w-full">
        <div class="flex-1 flex flex-col items-center justify-center bg-white rounded-xl shadow-sm border border-gray-200 p-12 text-center">
            <div class="rounded-full bg-amber-100 p-4 mb-6">
                <svg class="h-10 w-10 text-amber-600" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                </svg>
            </div>
            <h2 class="text-xl font-bold text-gray-900 mb-2">ビジュアライザURLが設定されていません</h2>
            <p class="text-gray-600 max-w-md mb-8">
                スコア分析機能を使用するには、設定タブでビジュアライザのURLを指定する必要があります。
                URLを設定することで、結果テーブルからケースの可視化が可能になります。
            </p>
            <button 
                onclick={goToSettings}
                class="inline-flex items-center px-6 py-3 border border-transparent text-base font-medium rounded-md shadow-sm text-white bg-indigo-600 hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-indigo-500 transition-all transform active:scale-95"
            >
                <svg class="-ml-1 mr-2 h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                </svg>
                設定へ移動
            </button>
        </div>
    </div>
  {:else}
    <div class="flex-1 flex overflow-hidden p-6 w-full">
        <div class="max-w-none w-full flex-1 flex flex-col relative bg-white rounded-xl shadow-sm border border-gray-200 overflow-hidden">
            <div class="absolute top-3 right-4 z-10">
                <button 
                onclick={reloadAnalysis}
                class="bg-white/90 hover:bg-white text-gray-700 px-3 py-1.5 rounded-lg shadow-sm text-sm border border-gray-300 transition-all active:scale-95 disabled:opacity-50"
                disabled={isLoading}
                >
                {isLoading ? 'Reloading...' : 'Reload Tool'}
                </button>
            </div>
            <iframe
            src={iframeSrc}
            title="Score Analysis"
            class="w-full h-full border-none"
            ></iframe>
        </div>
    </div>
  {/if}
</div>
