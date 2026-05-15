<script lang="ts" generics="T extends Record<string, any>">
	import type { Snippet } from 'svelte';

	type Column<T> = {
		// Usually a real data key on T; allow plain string so callers can declare
		// synthetic action/control columns (rendered via the cell snippet).
		key: (keyof T & string) | string;
		label: string;
		align?: 'left' | 'right' | 'center';
		sortable?: boolean;
		class?: string;
		widthClass?: string;
	};

	let {
		columns,
		rows,
		sortKey = $bindable(undefined),
		sortDir = $bindable('asc' as 'asc' | 'desc'),
		emptyMessage = 'No data',
		cell,
		fixedLayout = false,
		rowKeyFn,
		overlayKey = null,
		rowOverlay,
		class: className = ''
	}: {
		columns: Column<T>[];
		rows: T[];
		sortKey?: keyof T & string;
		sortDir?: 'asc' | 'desc';
		emptyMessage?: string;
		cell?: Snippet<[{ row: T; column: Column<T>; value: unknown }]>;
		fixedLayout?: boolean;
		// When `rowKeyFn(row) === overlayKey`, `rowOverlay` is rendered absolutely
		// across the row, on top of a backdrop that visually mutes the row content.
		rowKeyFn?: (row: T) => string;
		overlayKey?: string | null;
		rowOverlay?: Snippet<[{ row: T }]>;
		class?: string;
	} = $props();

	function handleSort(key: string) {
		if (sortKey === key) {
			sortDir = sortDir === 'asc' ? 'desc' : 'asc';
		} else {
			sortKey = key as keyof T & string;
			sortDir = 'asc';
		}
	}

	const alignClasses = {
		left: 'text-left',
		right: 'text-right',
		center: 'text-center'
	};
</script>

<div class="overflow-x-auto {className}">
	<table class="w-full text-sm {fixedLayout ? 'table-fixed' : ''}">
		{#if columns.some((col) => col.widthClass)}
			<colgroup>
				{#each columns as col}
					<col class={col.widthClass ?? ''} />
				{/each}
			</colgroup>
		{/if}
		<thead>
			<tr class="border-b border-border/70">
				{#each columns as col}
					<th
						class="eyebrow py-2.5 px-3
							{alignClasses[col.align ?? 'left']}
							{col.widthClass ?? ''}
							{col.sortable
							? 'cursor-pointer transition-colors duration-[var(--duration-fast)] hover:text-text'
							: ''}"
						onclick={col.sortable ? () => handleSort(col.key) : undefined}
						aria-sort={sortKey === col.key
							? sortDir === 'asc'
								? 'ascending'
								: 'descending'
							: undefined}
					>
						<span class="inline-flex items-center gap-1">
							{col.label}
							{#if col.sortable && sortKey === col.key}
								<span class="text-accent text-xs leading-none">
									{sortDir === 'asc' ? '↑' : '↓'}
								</span>
							{/if}
						</span>
					</th>
				{/each}
			</tr>
		</thead>
		<tbody>
			{#if rows.length === 0}
				<tr>
					<td
						colspan={columns.length}
						class="py-10 text-center text-sm text-text-tertiary"
					>
						{emptyMessage}
					</td>
				</tr>
			{:else}
				{#each rows as row}
					{@const isOverlaid =
						rowKeyFn !== undefined &&
						overlayKey !== null &&
						rowKeyFn(row) === overlayKey}
					<tr
						class="border-b border-border/30 transition-colors duration-[var(--duration-fast)]
							hover:bg-surface-hover/40
							{isOverlaid ? 'relative' : ''}"
					>
						{#each columns as col, ci}
							<td
								class="py-2.5 px-3 tabular-nums {alignClasses[col.align ?? 'left']} {col.class ?? ''}"
							>
								{#if cell}
									{@render cell({ row, column: col, value: row[col.key] })}
								{:else}
									{row[col.key]}
								{/if}
								{#if isOverlaid && rowOverlay && ci === columns.length - 1}
									<div
										class="absolute inset-0 z-10 flex items-center justify-center
											bg-surface/75 backdrop-blur-[1px]"
									>
										{@render rowOverlay({ row })}
									</div>
								{/if}
							</td>
						{/each}
					</tr>
				{/each}
			{/if}
		</tbody>
	</table>
</div>
