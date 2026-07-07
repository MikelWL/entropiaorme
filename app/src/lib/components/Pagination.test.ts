// @vitest-environment happy-dom

import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import Pagination from './Pagination.svelte';

describe('visibility', () => {
	it('renders nothing with a single page', () => {
		const { container } = render(Pagination, { props: { page: 0, totalPages: 1 } });
		expect(container.textContent?.trim()).toBe('');
		expect(screen.queryByRole('button')).toBeNull();
	});

	it('renders nothing with zero pages', () => {
		render(Pagination, { props: { page: 0, totalPages: 0 } });
		expect(screen.queryByRole('button')).toBeNull();
	});
});

describe('controls', () => {
	it('labels both buttons and shows the 1-based indicator', () => {
		render(Pagination, { props: { page: 1, totalPages: 5 } });

		expect(screen.getByLabelText('Previous page')).toBeTruthy();
		expect(screen.getByLabelText('Next page')).toBeTruthy();
		expect(screen.getByText('2 / 5')).toBeTruthy();
	});

	it('disables Prev on the first page only', () => {
		render(Pagination, { props: { page: 0, totalPages: 3 } });

		expect((screen.getByLabelText('Previous page') as HTMLButtonElement).disabled).toBe(true);
		expect((screen.getByLabelText('Next page') as HTMLButtonElement).disabled).toBe(false);
	});

	it('disables Next on the last page only', () => {
		render(Pagination, { props: { page: 2, totalPages: 3 } });

		expect((screen.getByLabelText('Previous page') as HTMLButtonElement).disabled).toBe(false);
		expect((screen.getByLabelText('Next page') as HTMLButtonElement).disabled).toBe(true);
	});

	it('steps the page through the buttons', async () => {
		render(Pagination, { props: { page: 1, totalPages: 5 } });

		await fireEvent.click(screen.getByLabelText('Next page'));
		expect(screen.getByText('3 / 5')).toBeTruthy();

		await fireEvent.click(screen.getByLabelText('Previous page'));
		await fireEvent.click(screen.getByLabelText('Previous page'));
		expect(screen.getByText('1 / 5')).toBeTruthy();
		expect((screen.getByLabelText('Previous page') as HTMLButtonElement).disabled).toBe(true);
	});

	it('shows the range label when given', () => {
		render(Pagination, { props: { page: 0, totalPages: 4, rangeLabel: '1-25 of 100' } });
		expect(screen.getByText('1-25 of 100')).toBeTruthy();
	});
});
