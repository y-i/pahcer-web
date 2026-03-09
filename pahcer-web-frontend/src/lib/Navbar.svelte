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

<nav class="bg-white border-b border-gray-200 z-50 relative">
  <div class="w-full px-6">
    <div class="flex justify-between h-14">
      <div class="flex items-center gap-8">
        <div class="flex-shrink-0 flex items-center">
          <span class="text-lg font-bold tracking-tight text-gray-900">
            Pahcer<span class="text-indigo-600">Web</span>
          </span>
        </div>
        <div class="hidden sm:flex sm:space-x-1 h-full">
          {#each tabs as tab}
            <button
              onclick={() => (activeTab = tab.id)}
              class="relative inline-flex items-center px-4 h-full text-sm font-medium transition-all duration-200 ease-out
                {activeTab === tab.id 
                  ? 'text-indigo-600' 
                  : 'text-gray-500 hover:text-gray-900 hover:bg-gray-50'}"
            >
              {tab.label}
              {#if activeTab === tab.id}
                <span class="absolute bottom-0 left-0 w-full h-0.5 bg-indigo-600 rounded-t-full"></span>
              {/if}
            </button>
          {/each}
        </div>
      </div>
      <!-- Right side placeholder for potential global actions -->
      <div class="flex items-center">
      </div>
    </div>
  </div>
</nav>
