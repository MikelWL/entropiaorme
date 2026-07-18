<script lang="ts">
	import type { Snippet } from 'svelte';

	interface MenuItem {
		label: string;
		danger?: boolean;
		onSelect: () => void;
	}

	let {
		ariaLabel = 'Open menu',
		items,
		trigger,
		children,
		class: className = '',
		panelClass = ''
	}: {
		/** Accessible name for the default three-dot trigger. */
		ariaLabel?: string;
		/** Declarative menu items; rendered before any custom panel content. */
		items?: MenuItem[];
		/**
		 * Custom trigger. When provided it replaces the default three-dot
		 * button; the snippet is responsible for wiring `toggle` to a button
		 * and carrying `aria-haspopup="menu"` / `aria-expanded`.
		 */
		trigger?: Snippet<[{ open: boolean; toggle: () => void }]>;
		/**
		 * Custom panel content, rendered after `items`. Elements marked
		 * `role="menuitem"` participate in the keyboard roving focus.
		 */
		children?: Snippet<[{ close: () => void }]>;
		class?: string;
		panelClass?: string;
	} = $props();

	let open = $state(false);
	let rootEl = $state<HTMLDivElement | null>(null);
	let panelEl = $state<HTMLDivElement | null>(null);
	let triggerEl = $state<HTMLButtonElement | null>(null);
	let openedFrom: HTMLElement | null = null;

	function menuItemEls(): HTMLElement[] {
		return panelEl ? Array.from(panelEl.querySelectorAll<HTMLElement>('[role="menuitem"]')) : [];
	}

	function focusItem(index: number) {
		const els = menuItemEls();
		if (els.length === 0) {
			// Custom panel content without menuitems: park focus on the panel
			// so Escape keeps working from inside the popover.
			panelEl?.focus();
			return;
		}
		const i = ((index % els.length) + els.length) % els.length;
		for (let j = 0; j < els.length; j++) {
			els[j].tabIndex = j === i ? 0 : -1;
		}
		els[i].focus();
	}

	function toggle() {
		if (open) {
			open = false;
		} else {
			openedFrom =
				triggerEl ?? (document.activeElement instanceof HTMLElement ? document.activeElement : null);
			open = true;
		}
	}

	function close(returnFocus: boolean) {
		open = false;
		if (returnFocus) {
			openedFrom?.focus();
		}
	}

	function handleTriggerKeydown(e: KeyboardEvent) {
		if (e.key === 'ArrowDown' && !open) {
			e.preventDefault();
			toggle();
		} else if (e.key === 'Escape' && open) {
			close(true);
		}
	}

	function handlePanelKeydown(e: KeyboardEvent) {
		const textEntry = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
		if (textEntry && ['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) return;
		const els = menuItemEls();
		const current = els.indexOf(document.activeElement as HTMLElement);
		switch (e.key) {
			case 'ArrowDown':
				e.preventDefault();
				focusItem(current + 1);
				break;
			case 'ArrowUp':
				e.preventDefault();
				focusItem(current - 1);
				break;
			case 'Home':
				e.preventDefault();
				focusItem(0);
				break;
			case 'End':
				e.preventDefault();
				focusItem(els.length - 1);
				break;
			case 'Escape':
				e.preventDefault();
				close(true);
				break;
			case 'Tab':
				// Menus do not trap focus: let Tab proceed and dismiss the popover.
				close(false);
				break;
		}
	}

	function handleItemClick(item: MenuItem) {
		close(false);
		item.onSelect();
	}

	// While open: focus the first menuitem and dismiss on any click outside
	// the component (listener lives only for the open lifetime).
	$effect(() => {
		if (!open) return;
		focusItem(0);
		const onDocumentClick = (e: MouseEvent) => {
			if (rootEl && e.target instanceof Node && !rootEl.contains(e.target)) {
				close(false);
			}
		};
		document.addEventListener('click', onDocumentClick, true);
		return () => document.removeEventListener('click', onDocumentClick, true);
	});
</script>

<div class="relative {className}" bind:this={rootEl}>
	{#if trigger}
		{@render trigger({ open, toggle })}
	{:else}
		<button
			bind:this={triggerEl}
			aria-label={ariaLabel}
			aria-haspopup="menu"
			aria-expanded={open}
			class="w-7 h-7 flex items-center justify-center rounded-md border border-border/50
				text-text-tertiary hover:text-text hover:bg-surface-hover transition-colors cursor-pointer"
			onclick={(e) => {
				e.stopPropagation();
				toggle();
			}}
			onkeydown={handleTriggerKeydown}
		>
			<svg class="w-3.5 h-3.5" fill="currentColor" viewBox="0 0 16 16">
				<circle cx="8" cy="3" r="1.5" /><circle cx="8" cy="8" r="1.5" /><circle
					cx="8"
					cy="13"
					r="1.5"
				/>
			</svg>
		</button>
	{/if}

	{#if open}
		<div
			bind:this={panelEl}
			role="menu"
			tabindex="-1"
			class="absolute right-0 top-8 z-20 bg-surface-raised border border-border rounded-md shadow-lg py-1 min-w-[100px] focus:outline-none {panelClass}"
			onkeydown={handlePanelKeydown}
		>
			{#each items ?? [] as item (item.label)}
				<button
					role="menuitem"
					tabindex="-1"
					class="w-full px-3 py-1.5 text-xs text-left text-text-secondary hover:bg-surface-hover {item.danger
						? 'hover:text-negative'
						: 'hover:text-text'} cursor-pointer"
					onclick={() => handleItemClick(item)}
				>
					{item.label}
				</button>
			{/each}
			{#if children}
				{@render children({ close: () => close(false) })}
			{/if}
		</div>
	{/if}
</div>
