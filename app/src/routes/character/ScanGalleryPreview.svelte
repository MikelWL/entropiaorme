<script lang="ts">
	let { src, page, onClose }: { src: string; page: number; onClose: () => void } = $props();

	function onKey(e: KeyboardEvent) {
		if (e.key === 'Escape') onClose();
	}

	$effect(() => {
		window.addEventListener('keydown', onKey);
		return () => window.removeEventListener('keydown', onKey);
	});
</script>

<div
	role="presentation"
	class="fixed inset-0 z-50 flex items-center justify-center bg-black/80 p-6"
	onclick={onClose}
>
	<div class="relative max-h-full max-w-full" role="presentation" onclick={(e) => e.stopPropagation()}>
		<img src={src} alt="Capture preview, page {page}" class="max-h-[90vh] max-w-[90vw] rounded-md shadow-2xl" />
		<div class="absolute left-2 top-2 rounded bg-black/60 px-2 py-1 text-xs text-text">
			Page {page}
		</div>
		<button
			class="absolute right-2 top-2 rounded bg-black/60 px-2 py-1 text-xs text-text hover:bg-black/80 cursor-pointer"
			onclick={onClose}
			aria-label="Close preview"
		>
			Close
		</button>
	</div>
</div>
