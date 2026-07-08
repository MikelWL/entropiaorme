/**
 * Guide-only DOM triggers for the armour-costs card's monitor SVG (the SVG
 * itself is authored in the dashboard guide-surface prose). Pure DOM: each
 * function targets the SVG's elements by id and no-ops when the card is not
 * on screen.
 */

export function triggerArmourDrag(): void {
	// CSS transition handles the slide; toggling `.docked` is enough.
	// `style.transition = ''` restores the class-defined transition in
	// case `resetArmourSvg` previously inlined `transition: none` for
	// an instant snap back to the start position.
	const win = document.getElementById('armour-svg-window');
	if (!win) return;
	win.style.transition = '';
	win.classList.add('docked');
}

export function triggerArmourFlash(): void {
	// Set animation via inline style + force-reflow trick so the
	// keyframe runs from frame 0 on every cursor-click iteration.
	// SVG elements lack `offsetWidth`; `getBoundingClientRect()` is
	// the SVG-compatible force-reflow primitive.
	const flash = document.getElementById('armour-svg-flash');
	if (!flash) return;
	flash.style.animation = 'none';
	flash.getBoundingClientRect();
	flash.style.animation = 'armourFlash 500ms ease-out';
}

export function resetArmourSvg(): void {
	// Snap window back to start (no animation) and clear any in-flight
	// flash. Done in the loop's gap phase so the next iteration's drag
	// reads as a fresh "place the terminal" beat, not a slow revert.
	const win = document.getElementById('armour-svg-window');
	if (win) {
		win.style.transition = 'none';
		win.classList.remove('docked');
		win.getBoundingClientRect();
		win.style.transition = '';
	}
	const flash = document.getElementById('armour-svg-flash');
	if (flash) flash.style.animation = 'none';
}
