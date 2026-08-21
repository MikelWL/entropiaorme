// @vitest-environment happy-dom

import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const api = vi.hoisted(() => ({ updateSettings: vi.fn() }));
vi.mock('$lib/api', () => api);

import EffectsTab from './EffectsTab.svelte';

beforeEach(() => vi.clearAllMocks());

describe('persistent effects', () => {
	it('saves a named reload-speed source through the typed settings boundary', async () => {
		const onchange = vi.fn();
		api.updateSettings.mockResolvedValue({
			passiveEffectSources: [
				{
					id: 'ares-perfect',
					name: 'Ares Ring, Perfected',
					enabled: true,
					effects: [{ kind: 'reload_speed', magnitudePercent: 14 }],
				},
			],
		});
		render(EffectsTab, { props: { sources: [], onchange } });

		await fireEvent.click(screen.getByText('Add effect'));
		await fireEvent.input(screen.getByPlaceholderText('Ares Ring, Perfected'), {
			target: { value: 'Ares Ring, Perfected' },
		});
		await fireEvent.input(screen.getByRole('spinbutton'), { target: { value: '14' } });
		await fireEvent.click(screen.getByText('Save effects'));

		await waitFor(() =>
			expect(api.updateSettings).toHaveBeenCalledWith({
				passive_effect_sources: [
					expect.objectContaining({
						name: 'Ares Ring, Perfected',
						enabled: true,
						effects: [{ kind: 'reload_speed', magnitude_percent: 14 }],
					}),
				],
			}),
		);
		expect(onchange).toHaveBeenCalledWith([
			expect.objectContaining({ name: 'Ares Ring, Perfected', enabled: true }),
		]);
	});

	it('does not save unnamed or invalid combined sources', async () => {
		render(EffectsTab, { props: { sources: [] } });
		await fireEvent.click(screen.getByText('Add effect'));

		const save = screen.getByText('Save effects') as HTMLButtonElement;
		expect(save.disabled).toBe(true);
		expect(screen.getByText(/Name every source/)).toBeTruthy();
		expect(api.updateSettings).not.toHaveBeenCalled();
	});
});
