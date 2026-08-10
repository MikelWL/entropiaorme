<script lang="ts">
	let {
		letter,
		label,
		onclick,
		disabled = false,
		title = '',
	}: {
		letter: string;
		label: string;
		onclick: () => void;
		disabled?: boolean;
		title?: string;
	} = $props();
</script>

<!-- The expanded width comes from the label rather than a number chosen to
	suit it: `ch` is the font's own digit advance, so this tracks the type it is
	measuring and cannot be left behind by a rename. Lowercase letters run
	narrower than a digit, so it errs on the side of fitting. -->
<button
	type="button"
	{onclick}
	{disabled}
	{title}
	aria-label={label}
	style="--expanded: calc({label.length}ch + 1.25rem)"
	class="group/act relative inline-flex h-6 w-6 shrink-0 items-center justify-center overflow-hidden
		rounded-md border border-border/40 bg-transparent text-xs font-semibold text-text-secondary
		transition-[width,color,border-color] duration-[var(--duration-base)] ease-[var(--ease-out)]
		hover:w-[var(--expanded)] hover:border-border hover:text-text
		focus-visible:w-[var(--expanded)] focus-visible:border-border focus-visible:text-text
		disabled:cursor-not-allowed disabled:border-dashed disabled:text-text-tertiary
		disabled:hover:border-border/40 disabled:hover:text-text-tertiary"
>
	<span
		class="absolute inset-0 flex items-center justify-center
			transition-opacity duration-[var(--duration-fast)] group-hover/act:opacity-0
			group-focus-visible/act:opacity-0"
	>
		{letter}
	</span>
	<span
		class="absolute inset-0 flex items-center justify-center whitespace-nowrap px-2
			opacity-0 transition-opacity duration-[var(--duration-fast)] group-hover/act:opacity-100
			group-focus-visible/act:opacity-100"
	>
		{label}
	</span>
</button>
