<script lang="ts">
	import type { Snippet } from 'svelte';
	import { fade, scale } from 'svelte/transition';
	import { quintOut } from 'svelte/easing';

	let {
		open = $bindable(false),
		title,
		children,
		class: className = ''
	}: {
		open?: boolean;
		title?: string;
		children: Snippet;
		class?: string;
	} = $props();

	function handleBackdropClick(e: MouseEvent) {
		if (e.target === e.currentTarget) {
			open = false;
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			open = false;
		}
	}
</script>

<svelte:window onkeydown={open ? handleKeydown : undefined} />

{#if open}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="fixed inset-0 z-50 flex items-center justify-center p-6"
		onclick={handleBackdropClick}
		onkeydown={handleKeydown}
	>
		<div
			class="absolute inset-0 bg-base/70 backdrop-blur-sm"
			transition:fade={{ duration: 180 }}
		></div>

		<div
			class="relative z-10 w-full max-w-md rounded-lg border border-border-bright/60
				bg-surface-raised/95 shadow-lg backdrop-blur-md
				before:pointer-events-none before:absolute before:inset-0 before:rounded-[inherit]
				before:[box-shadow:inset_0_1px_0_0_rgba(255,255,255,0.05)]
				p-6 {className}"
			role="dialog"
			aria-modal="true"
			aria-label={title}
			transition:scale={{ duration: 220, start: 0.96, easing: quintOut }}
		>
			{#if title}
				<div class="relative flex items-center justify-between mb-5 pb-4 border-b border-border/50">
					<h2 class="text-base font-semibold text-text tracking-tight">{title}</h2>
					<button
						class="h-7 w-7 flex items-center justify-center rounded-md
							text-text-tertiary cursor-pointer
							transition-colors duration-[var(--duration-fast)]
							hover:text-text hover:bg-surface-hover"
						onclick={() => (open = false)}
						aria-label="Close"
					>
						<svg
							xmlns="http://www.w3.org/2000/svg"
							viewBox="0 0 20 20"
							fill="currentColor"
							class="h-4 w-4"
						>
							<path
								d="M6.28 5.22a.75.75 0 00-1.06 1.06L8.94 10l-3.72 3.72a.75.75 0 101.06 1.06L10 11.06l3.72 3.72a.75.75 0 101.06-1.06L11.06 10l3.72-3.72a.75.75 0 00-1.06-1.06L10 8.94 6.28 5.22z"
							/>
						</svg>
					</button>
				</div>
			{/if}

			<div class="relative">
				{@render children()}
			</div>
		</div>
	</div>
{/if}
