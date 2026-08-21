<script lang="ts">
	import { assignSessionProtectionLoadout, type ProtectionOverview } from '$lib/api';

	interface Props {
		sessionId: string;
		protection: ProtectionOverview | null;
		onassigned: (loadoutId: string) => void;
	}

	let { sessionId, protection, onassigned }: Props = $props();

	let assigning = $state(false);
	let error = $state<string | null>(null);

	async function choose(loadoutId: string) {
		if (!protection || assigning) return;
		assigning = true;
		error = null;
		try {
			await assignSessionProtectionLoadout(sessionId, loadoutId);
			onassigned(loadoutId);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Armour setup could not be saved';
		} finally {
			assigning = false;
		}
	}
</script>

<div class="flex min-w-[390px] flex-col gap-3 text-white">
	<div class="border-b border-white/10 pb-2.5">
		<p class="text-xs font-semibold">Armour used</p>
		<p class="mt-1 text-[11px] text-white/45">Choose the setup worn during this session. Its cost will stay at whole-session level.</p>
	</div>
	<div class="flex items-center gap-2">
		<select
			class="min-w-64 rounded-[4px] border border-white/15 bg-white/5 px-2.5 py-1.5 text-xs text-white outline-none focus:border-accent/60"
			disabled={assigning}
			onchange={(event) => {
				if (event.currentTarget.value) void choose(event.currentTarget.value);
			}}
		>
			<option value="">Choose armour setup</option>
			{#each protection?.loadouts ?? [] as loadout (loadout.id)}
				<option value={loadout.id}>{loadout.name}</option>
			{/each}
		</select>
		{#if assigning}<span class="text-[10px] text-white/40">Saving...</span>{/if}
	</div>
	{#if error}<p class="border-l-2 border-amber-400/70 pl-2 text-[10px] text-amber-200/80">{error}</p>{/if}
</div>
