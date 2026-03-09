<script lang="ts">
  import Navbar from './lib/Navbar.svelte';
  import TestRun from './lib/TestRun.svelte';
  import History from './lib/History.svelte';
  import ScoreAnalysis from './lib/ScoreAnalysis.svelte';
  import Settings from './lib/Settings.svelte';

  const tabs = [
    { id: 'test-run', label: 'テスト実行' },
    { id: 'history', label: '評価履歴' },
    { id: 'score-analysis', label: 'スコア分析' },
    { id: 'settings', label: '設定' },
  ];

  function getTabFromPath() {
    const path = window.location.pathname.slice(1);
    return tabs.find(t => t.id === path)?.id ?? 'test-run';
  }

  let activeTab = $state(getTabFromPath());

  $effect(() => {
    const handlePopState = () => {
      activeTab = getTabFromPath();
    };
    window.addEventListener('popstate', handlePopState);
    return () => {
      window.removeEventListener('popstate', handlePopState);
    };
  });

  $effect(() => {
    const path = window.location.pathname.slice(1);
    const currentTab = path === '' ? 'test-run' : path;
    
    if (activeTab !== currentTab) {
      const newPath = activeTab === 'test-run' ? '/' : `/${activeTab}`;
      history.pushState(null, '', newPath);
    }
  });
</script>

<div class="min-h-screen bg-gray-50 flex flex-col">
  <Navbar {tabs} bind:activeTab />

  <main class="flex-1 overflow-hidden flex flex-col">
    {#if activeTab === 'test-run'}
      <TestRun />
    {:else if activeTab === 'history'}
      <History />
    {:else if activeTab === 'score-analysis'}
      <ScoreAnalysis />
    {:else if activeTab === 'settings'}
      <Settings />
    {/if}
  </main>
</div>

<style>
  :global(body) {
    margin: 0;
    padding: 0;
  }
</style>