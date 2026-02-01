<script lang="ts">
  interface Tab {
    id: string;
    label: string;
  }

  let { tabs, activeTab = $bindable() } = $props<{
    tabs: Tab[];
    activeTab: string;
  }>();
</script>

<nav class="bg-white border-b border-gray-200 shadow-sm">
  <div class="max-w-7xl mx-auto px-4">
    <div class="flex justify-between h-16">
      <div class="flex">
        <div class="flex-shrink-0 flex items-center mr-8">
          <span class="text-xl font-bold text-indigo-600">Pahcer Web</span>
        </div>
        <div class="hidden sm:-my-px sm:ml-6 sm:flex sm:space-x-8">
          {#each tabs as tab}
            <button
              onclick={() => (activeTab = tab.id)}
              class="inline-flex items-center px-1 pt-1 border-b-2 text-sm font-medium transition-colors {activeTab === tab.id ? 'border-indigo-500 text-gray-900' : 'border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300'}"
            >
              {tab.label}
            </button>
          {/each}
        </div>
      </div>
    </div>
  </div>
</nav>
