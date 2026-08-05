// @vitest-environment happy-dom

import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import Menu from './Menu.svelte';

function threeItems(overrides: { onEdit?: () => void; onDelete?: () => void } = {}) {
	return [
		{ label: 'Edit', onSelect: overrides.onEdit ?? vi.fn() },
		{ label: 'Duplicate', onSelect: vi.fn() },
		{ label: 'Delete', danger: true, onSelect: overrides.onDelete ?? vi.fn() },
	];
}

async function openMenu(ariaLabel = 'Quest actions') {
	const trigger = screen.getByLabelText(ariaLabel);
	await fireEvent.click(trigger);
	return trigger;
}

describe('trigger', () => {
	it('renders the default trigger closed, with menu button semantics', () => {
		render(Menu, { props: { ariaLabel: 'Quest actions', items: threeItems() } });

		const trigger = screen.getByLabelText('Quest actions');
		expect(trigger.getAttribute('aria-haspopup')).toBe('menu');
		expect(trigger.getAttribute('aria-expanded')).toBe('false');
		expect(screen.queryByRole('menu')).toBeNull();
	});

	it('opens the panel on click and reflects the expanded state', async () => {
		render(Menu, { props: { ariaLabel: 'Quest actions', items: threeItems() } });

		const trigger = await openMenu();
		expect(trigger.getAttribute('aria-expanded')).toBe('true');
		expect(screen.getByRole('menu')).toBeTruthy();
		expect(screen.getAllByRole('menuitem').map((el) => el.textContent?.trim())).toEqual([
			'Edit',
			'Duplicate',
			'Delete',
		]);
	});
});

describe('keyboard navigation', () => {
	it('focuses the first item on open and cycles with ArrowDown / ArrowUp', async () => {
		render(Menu, { props: { ariaLabel: 'Quest actions', items: threeItems() } });
		await openMenu();

		const items = screen.getAllByRole('menuitem');
		expect(document.activeElement).toBe(items[0]);
		// Roving tabindex: only the active item is in the tab order.
		expect(items.map((el) => el.tabIndex)).toEqual([0, -1, -1]);

		const menu = screen.getByRole('menu');
		await fireEvent.keyDown(menu, { key: 'ArrowDown' });
		expect(document.activeElement).toBe(items[1]);
		await fireEvent.keyDown(menu, { key: 'ArrowDown' });
		expect(document.activeElement).toBe(items[2]);
		// Wraps past the end.
		await fireEvent.keyDown(menu, { key: 'ArrowDown' });
		expect(document.activeElement).toBe(items[0]);
		// And back up, wrapping to the tail.
		await fireEvent.keyDown(menu, { key: 'ArrowUp' });
		expect(document.activeElement).toBe(items[2]);
		expect(items.map((el) => el.tabIndex)).toEqual([-1, -1, 0]);
	});

	it('jumps with Home and End', async () => {
		render(Menu, { props: { ariaLabel: 'Quest actions', items: threeItems() } });
		await openMenu();

		const items = screen.getAllByRole('menuitem');
		const menu = screen.getByRole('menu');
		await fireEvent.keyDown(menu, { key: 'End' });
		expect(document.activeElement).toBe(items[2]);
		await fireEvent.keyDown(menu, { key: 'Home' });
		expect(document.activeElement).toBe(items[0]);
	});

	it('closes on Escape and returns focus to the trigger', async () => {
		render(Menu, { props: { ariaLabel: 'Quest actions', items: threeItems() } });
		const trigger = await openMenu();

		await fireEvent.keyDown(screen.getByRole('menu'), { key: 'Escape' });
		expect(screen.queryByRole('menu')).toBeNull();
		expect(trigger.getAttribute('aria-expanded')).toBe('false');
		expect(document.activeElement).toBe(trigger);
	});
});

describe('dismissal and activation', () => {
	it('closes on a click outside the component', async () => {
		render(Menu, { props: { ariaLabel: 'Quest actions', items: threeItems() } });
		await openMenu();

		await fireEvent.click(document.body);
		expect(screen.queryByRole('menu')).toBeNull();
	});

	it('stays open on a click inside the panel that is not an item', async () => {
		render(Menu, { props: { ariaLabel: 'Quest actions', items: threeItems() } });
		await openMenu();

		await fireEvent.click(screen.getByRole('menu'));
		expect(screen.getByRole('menu')).toBeTruthy();
	});

	it('runs the item action and closes on selection', async () => {
		const onEdit = vi.fn();
		render(Menu, { props: { ariaLabel: 'Quest actions', items: threeItems({ onEdit }) } });
		const trigger = await openMenu();

		await fireEvent.click(screen.getByRole('menuitem', { name: 'Edit' }));
		expect(onEdit).toHaveBeenCalledTimes(1);
		expect(screen.queryByRole('menu')).toBeNull();
		expect(document.activeElement).toBe(trigger);
	});
});

// A panel laid out inside a scrollable ancestor is clipped by it, and
// grows that ancestor's scroll extent to fit: the list appears to be
// squashed to make room rather than the menu floating over it. The
// overlay mode positions against the viewport to escape that, so what
// matters is that it stops taking part in the ancestor's layout.
describe('overlay positioning', () => {
	it('lays the default panel out in flow, absolutely positioned to the trigger', async () => {
		render(Menu, { props: { ariaLabel: 'Quest actions', items: threeItems() } });
		await openMenu();

		const panel = screen.getByRole('menu');
		expect(panel.className).toContain('absolute');
		expect(panel.className).not.toContain('fixed');
	});

	it('takes the panel out of the ancestor scroll box when overlaid', async () => {
		render(Menu, {
			props: { ariaLabel: 'Quest actions', items: threeItems(), overlay: true },
		});
		await openMenu();

		const panel = screen.getByRole('menu');
		expect(panel.className).toContain('fixed');
		// The trigger-relative offsets go with it: an overlay panel is
		// placed by measurement, not by utility classes.
		expect(panel.className).not.toContain('absolute');
		expect(panel.className).not.toContain('top-8');
		expect(panel.getAttribute('style')).toMatch(/top:\s*-?\d/);
		expect(panel.getAttribute('style')).toMatch(/left:\s*-?\d/);
	});

	it('keeps the panel clear of the viewport edges', async () => {
		render(Menu, {
			props: { ariaLabel: 'Quest actions', items: threeItems(), overlay: true },
		});
		await openMenu();

		const style = screen.getByRole('menu').getAttribute('style') ?? '';
		const top = Number(/top:\s*(-?\d+(?:\.\d+)?)px/.exec(style)?.[1]);
		const left = Number(/left:\s*(-?\d+(?:\.\d+)?)px/.exec(style)?.[1]);
		expect(top).toBeGreaterThanOrEqual(0);
		expect(left).toBeGreaterThanOrEqual(0);
	});
});
