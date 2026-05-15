<script lang="ts">
	let {
		class: className = ''
	}: {
		class?: string;
	} = $props();
</script>

<hr
	class="border-0 h-px bg-gradient-to-r from-transparent via-border to-transparent {className}"
/>
