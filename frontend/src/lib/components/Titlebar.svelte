<script lang="ts">
	import { getCurrentWindow } from '@tauri-apps/api/window';

	const win = getCurrentWindow();

	const minimize = () => win.minimize();
	const toggleMaximize = () => win.toggleMaximize();
	const close = () => win.close();
</script>

<div class="titlebar" data-tauri-drag-region>
	<div class="controls">
		<button type="button" class="ctrl" aria-label="Minimise" onclick={minimize}>
			<svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1">
				<line x1="1" y1="5" x2="9" y2="5" />
			</svg>
		</button>
		<button type="button" class="ctrl" aria-label="Maximise" onclick={toggleMaximize}>
			<svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1">
				<rect x="1" y="1" width="8" height="8" />
			</svg>
		</button>
		<button type="button" class="ctrl ctrl-close" aria-label="Close" onclick={close}>
			<svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1">
				<line x1="1" y1="1" x2="9" y2="9" />
				<line x1="9" y1="1" x2="1" y2="9" />
			</svg>
		</button>
	</div>
</div>

<style>
	.titlebar {
		height: 32px;
		flex-shrink: 0;
		background: var(--color-base);
		display: flex;
		align-items: stretch;
		justify-content: flex-end;
		user-select: none;
		-webkit-user-select: none;
	}
	.controls {
		display: flex;
	}
	.ctrl {
		width: 46px;
		height: 100%;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--color-text-tertiary);
		background: transparent;
		border: 0;
		cursor: pointer;
		transition: background-color var(--duration-base) var(--ease-out),
			color var(--duration-base) var(--ease-out);
	}
	.ctrl:hover {
		background: var(--color-surface-hover);
		color: var(--color-text);
	}
	.ctrl-close:hover {
		background: var(--color-error);
		color: var(--color-text);
	}
	.ctrl:focus-visible {
		outline: 1px solid var(--color-accent);
		outline-offset: -2px;
	}
</style>
