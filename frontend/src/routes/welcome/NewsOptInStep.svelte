<script lang="ts">
	let { optedIn = $bindable(false) } = $props<{ optedIn?: boolean }>();
</script>

<div class="block">
	<header class="step-header">
		<span class="eyebrow">Optional download</span>
		<h2 class="step-title">News &amp; Updates.</h2>
	</header>
	<p class="body">
		EntropiaOrme is offline by default in both directions: nothing leaves your machine, and the app does not download anything from the open internet.
	</p>
	<p class="body">
		Opt in below if you'd like the app to pull project articles and release notices from <code>entropiaorme.com</code> on launch.
	</p>
	<label class="toggle">
		<input type="checkbox" bind:checked={optedIn} />
		<span class="toggle-text">Enable News &amp; Updates.</span>
	</label>
	<span class="footnote">You can change this any time from Settings.</span>
</div>

<style>
	.block {
		display: grid;
		gap: 1.25rem;
	}
	.step-header {
		display: flex;
		flex-direction: column;
		gap: 0.625rem;
	}
	.eyebrow {
		font-size: 0.6875rem;
		letter-spacing: 0.22em;
		text-transform: uppercase;
		color: var(--color-text-tertiary);
		font-weight: 500;
	}
	.step-title {
		font-size: 1.5rem;
		line-height: 1.2;
		font-weight: 500;
		letter-spacing: -0.012em;
		color: var(--color-text);
		margin: 0;
	}
	.body {
		margin: 0;
		font-size: 0.9rem;
		line-height: 1.6;
		color: var(--color-text-secondary);
		max-width: 46ch;
	}
	.body :global(code) {
		font-family: var(--font-mono, ui-monospace, 'Cascadia Code', monospace);
		font-size: 0.85em;
		padding: 0.05rem 0.3rem;
		border-radius: var(--radius-sm, 4px);
		background: color-mix(in oklab, var(--color-surface) 60%, transparent);
		border: 1px solid color-mix(in oklab, var(--color-border) 60%, transparent);
		color: var(--color-text);
	}
	.toggle {
		display: flex;
		align-items: flex-start;
		gap: 0.625rem;
		padding: 0.875rem 1rem;
		border: 1px solid color-mix(in oklab, var(--color-border) 90%, transparent);
		border-radius: var(--radius-md);
		background: color-mix(in oklab, var(--color-surface) 50%, transparent);
		cursor: pointer;
		transition:
			border-color var(--duration-base) var(--ease-out),
			background var(--duration-base) var(--ease-out);
	}
	.toggle:hover {
		border-color: color-mix(in oklab, var(--color-accent) 35%, var(--color-border));
		background: color-mix(in oklab, var(--color-surface-hover) 70%, transparent);
	}
	.toggle input[type='checkbox'] {
		flex-shrink: 0;
		margin-top: 0.1875rem;
		width: 0.9375rem;
		height: 0.9375rem;
		accent-color: var(--color-accent);
		cursor: pointer;
	}
	.toggle-text {
		font-size: 0.8125rem;
		line-height: 1.55;
		color: var(--color-text);
	}
	.footnote {
		font-size: 0.75rem;
		letter-spacing: 0.04em;
		color: var(--color-text-tertiary);
	}
</style>
