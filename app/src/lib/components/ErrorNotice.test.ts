// @vitest-environment happy-dom

import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import ErrorNotice from './ErrorNotice.svelte';

describe('rendering', () => {
	it('renders nothing when the message is null', () => {
		const { container } = render(ErrorNotice, { props: { message: null } });
		expect(container.textContent?.trim()).toBe('');
		expect(screen.queryByRole('alert')).toBeNull();
	});

	it('renders the message as an alert with the house error strip styling', () => {
		render(ErrorNotice, { props: { message: 'Failed to load quests' } });

		const alert = screen.getByRole('alert');
		expect(alert.textContent?.trim()).toBe('Failed to load quests');
		expect(alert.className).toContain('text-negative');
		expect(alert.className).toContain('bg-negative/10');
	});

	it('omits the dismiss control without a handler', () => {
		render(ErrorNotice, { props: { message: 'Failed to load quests' } });
		expect(screen.queryByText('Dismiss')).toBeNull();
	});
});

describe('dismissal', () => {
	it('renders a Dismiss linklet that fires the handler', async () => {
		const onDismiss = vi.fn();
		render(ErrorNotice, { props: { message: 'Failed to load quests', onDismiss } });

		const dismiss = screen.getByText('Dismiss');
		expect(dismiss.className).toContain('linklet');
		await fireEvent.click(dismiss);
		expect(onDismiss).toHaveBeenCalledTimes(1);
	});
});
