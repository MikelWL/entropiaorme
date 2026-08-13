<script lang="ts">
	import Button from '$lib/components/Button.svelte';
	import Card from '$lib/components/Card.svelte';
	import DataTable from '$lib/components/DataTable.svelte';
	import ErrorNotice from '$lib/components/ErrorNotice.svelte';
	import type { MarketPastePreviewRow } from '$lib/api';
	import { createImportModel } from '$lib/features/market/importModel.svelte';

	let { onimported }: { onimported: () => void } = $props();

	const model = createImportModel();

	const previewColumns = [
		{ key: 'itemName', label: 'Item' },
		{ key: 'day', label: 'Day', align: 'right' as const },
		{ key: 'week', label: 'Week', align: 'right' as const },
		{ key: 'month', label: 'Month', align: 'right' as const },
		{ key: 'year', label: 'Year', align: 'right' as const },
		{ key: 'decade', label: 'Decade', align: 'right' as const }
	];

	function markupOf(row: MarketPastePreviewRow, key: string): number | null {
		switch (key) {
			case 'day':
				return row.day.markupPct;
			case 'week':
				return row.week.markupPct;
			case 'month':
				return row.month.markupPct;
			case 'year':
				return row.year.markupPct;
			case 'decade':
				return row.decade.markupPct;
			default:
				return null;
		}
	}

	async function handleCommit() {
		if (await model.commit()) onimported();
	}

	async function handleUnitPriceSave() {
		await model.saveUnitPrice();
	}
</script>

<div class="space-y-4">
	<Card>
		<div class="space-y-3">
			<p class="text-sm text-text-secondary">
				In game, add the items you track to the market ledger (right-click an item, view market
				data), copy the ledger, and paste it here.
			</p>
			<textarea
				value={model.text}
				oninput={(e) => model.setText(e.currentTarget.value)}
				rows="7"
				spellcheck="false"
				placeholder="Paste the market-ledger export"
				aria-label="Market-ledger paste"
				class="w-full px-3 py-2 text-sm font-mono bg-surface/70 text-text rounded-md border border-border placeholder:text-text-tertiary transition-[border-color,box-shadow,background-color] duration-[var(--duration-base)] ease-[var(--ease-out)] hover:border-border-bright focus:outline-none focus:bg-surface focus:border-accent/60 focus:[box-shadow:var(--shadow-glow)] resize-y"
			></textarea>
			<div class="flex items-center gap-2">
				<Button
					variant="secondary"
					size="sm"
					disabled={!model.canPreview}
					loading={model.previewing}
					onclick={() => void model.runPreview()}
				>
					Preview
				</Button>
				<Button
					variant="primary"
					size="sm"
					disabled={!model.canCommit}
					loading={model.committing}
					onclick={() => void handleCommit()}
				>
					Import
				</Button>
				{#if model.preview && model.preview.rows.length > 0}
					<span class="text-sm text-text-secondary">
						{model.preview.rows.length}
						{model.preview.rows.length === 1 ? 'item' : 'items'} ready{model.preview.skipped
							.length > 0
							? `, ${model.preview.skipped.length} skipped`
							: ''}
					</span>
				{/if}
			</div>

			<div class="border-t border-border/70 pt-4 space-y-3">
				<div>
					<h2 class="text-sm font-medium text-text">Absolute item value</h2>
					<p class="mt-1 text-sm text-text-secondary">
						Use a PED-per-unit quote for zero-TT or unit-priced items. It remains estimated
						market value, never realised gains.
					</p>
				</div>
				<div class="grid gap-3 sm:grid-cols-[minmax(0,1fr)_10rem_auto] sm:items-end">
					<label class="block">
						<span class="mb-1 block text-xs font-medium text-text-secondary">Item</span>
						<input
							value={model.unitItemName}
							disabled={model.unitPriceSaving}
							oninput={(event) => model.setUnitItemName(event.currentTarget.value)}
							placeholder="Item name"
							class="w-full rounded-md border border-border bg-surface/70 px-3 py-2 text-sm text-text focus:border-accent/60 focus:outline-none focus:[box-shadow:var(--shadow-glow)]"
						/>
					</label>
					<label class="block">
						<span class="mb-1 block text-xs font-medium text-text-secondary">PED per unit</span>
						<input
							type="number"
							disabled={model.unitPriceSaving}
							min="0"
							step="0.01"
							value={model.unitPriceInput}
							oninput={(event) => model.setUnitPriceInput(event.currentTarget.value)}
							placeholder="0.00"
							class="w-full rounded-md border border-border bg-surface/70 px-3 py-2 text-sm tabular-nums text-text focus:border-accent/60 focus:outline-none focus:[box-shadow:var(--shadow-glow)]"
						/>
					</label>
					<Button
						variant="secondary"
						size="sm"
						disabled={!model.canSaveUnitPrice}
						loading={model.unitPriceSaving}
						onclick={() => void handleUnitPriceSave()}
					>
						Save quote
					</Button>
				</div>
				{#if model.unitPriceError}
					<ErrorNotice message={model.unitPriceError} />
				{:else if model.unitPriceSaved}
					<p class="text-sm text-success">
						Saved {model.unitPriceSaved.itemName} at {model.unitPriceSaved.pedPerUnit.toFixed(2)}
						PED per unit.
					</p>
				{/if}
			</div>
		</div>
	</Card>

	{#if model.error}
		<ErrorNotice message={model.error} />
	{/if}

	{#if model.preview && model.preview.rows.length > 0}
		<Card>
			<h2 class="text-sm font-medium text-text mb-3">Markup by window</h2>
			<DataTable
				columns={previewColumns}
				rows={model.preview.rows}
				emptyMessage="Nothing parsed"
			>
				{#snippet cell({ row, column }: { row: MarketPastePreviewRow; column: { key: string } })}
					{#if column.key === 'itemName'}
						<span class="text-text">{row.itemName}</span>
					{:else}
						{@const markup = markupOf(row, column.key)}
						{#if markup === null}
							<span class="text-text-tertiary">N/A</span>
						{:else}
							<span class="tabular-nums">{markup.toFixed(2)}%</span>
						{/if}
					{/if}
				{/snippet}
			</DataTable>
		</Card>
	{/if}

	{#if model.preview && model.preview.skipped.length > 0}
		<Card>
			<h2 class="text-sm font-medium text-text mb-2">Skipped lines</h2>
			<ul class="space-y-1.5">
				{#each model.preview.skipped as line (line.lineNumber)}
					<li class="text-sm">
						<span class="text-text-tertiary">Line {line.lineNumber}:</span>
						<span class="text-text-secondary">{line.reason}</span>
						<code class="block mt-0.5 text-xs text-text-tertiary truncate">{line.content}</code>
					</li>
				{/each}
			</ul>
		</Card>
	{/if}
</div>
