// @vitest-environment happy-dom

import { render, screen } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { StatId } from '$lib/statsRegistry';
import { getStatDef } from '$lib/statsRegistry';

// The strip renders from props plus the overlayStats customisation state; the
// state module is the one side-effecting seam (the real module pulls in the
// Tauri preference plumbing), so it is replaced with a plain `{ current }`
// stub the tests assign before each render (vi.hoisted so the mock factory
// can reference it before top-level imports initialise). The stats registry
// is real: the pill assertions exercise the actual render functions.
const { overlayStats } = vi.hoisted(() => {
	type Pref = { id: string; enabled: boolean };
	return {
		overlayStats: { current: [] as Pref[] },
	};
});

vi.mock('$lib/statsCustomisation.svelte', () => ({
	overlayStats,
}));

import type { TrackingLive, TrackingStatus } from '$lib/api';
import OverlayStrip from './OverlayStrip.svelte';

function liveData(overrides: Partial<TrackingLive> = {}): TrackingLive {
	return { status: 'idle', ...overrides };
}

function activeStatus(overrides: Partial<TrackingStatus> = {}): TrackingStatus {
	return { status: 'active', ...overrides };
}

beforeEach(() => {
	overlayStats.current = [];
});

describe('track / stop control', () => {
	it('renders TRACK when idle and forwards the click to onStart', async () => {
		const onStart = vi.fn();
		render(OverlayStrip, { props: { data: liveData(), onStart } });

		const button = screen.getByTitle('Start tracking');
		expect(button.textContent).toContain('TRACK');
		button.click();
		expect(onStart).toHaveBeenCalledTimes(1);
	});

	it('renders the stop control with the elapsed timer when active', async () => {
		const onStop = vi.fn();
		render(OverlayStrip, {
			props: { data: liveData({ status: 'active', elapsed: 3725 }), onStop },
		});

		const button = screen.getByTitle('Stop tracking');
		button.click();
		expect(onStop).toHaveBeenCalledTimes(1);
		expect(screen.queryByTitle('Start tracking')).toBeNull();
		// 3725s formats as h:mm:ss with zero-padded minutes and seconds.
		expect(screen.getByText('1:02:05')).toBeTruthy();
	});

	it('formats a sub-hour elapsed as m:ss', () => {
		render(OverlayStrip, { props: { data: liveData({ status: 'active', elapsed: 65 }) } });
		expect(screen.getByText('1:05')).toBeTruthy();
	});

	it('disables the control and shows a busy marker while toggling', () => {
		render(OverlayStrip, { props: { data: liveData(), toggling: true } });
		const button = screen.getByTitle('Start tracking') as HTMLButtonElement;
		expect(button.disabled).toBe(true);
		expect(button.textContent).toContain('...');
	});
});

describe('armour track decision prompt', () => {
	it('replaces the stop control during an active session and forwards the decision', () => {
		const onArmourTrackDecision = vi.fn();
		render(OverlayStrip, {
			props: {
				data: liveData({ status: 'active' }),
				awaitingArmourTrackDecision: true,
				onArmourTrackDecision,
			},
		});

		expect(screen.getByText('Track armour?')).toBeTruthy();
		expect(screen.queryByTitle('Stop tracking')).toBeNull();

		screen.getByText('Yes').click();
		expect(onArmourTrackDecision).toHaveBeenCalledWith('yes');
		screen.getByText('No').click();
		expect(onArmourTrackDecision).toHaveBeenCalledWith('no');
	});

	it('does not interpose when the session is not active', () => {
		render(OverlayStrip, {
			props: { data: liveData(), awaitingArmourTrackDecision: true },
		});
		expect(screen.queryByText('Track armour?')).toBeNull();
		expect(screen.getByTitle('Start tracking')).toBeTruthy();
	});
});

describe('attribution warning', () => {
	it('replaces TRACK while idle and dismisses through the callback', () => {
		const onDismissAttributionWarning = vi.fn();
		render(OverlayStrip, {
			props: {
				data: liveData(),
				attributionWarning: 'Configure a weapon before tracking',
				onDismissAttributionWarning,
			},
		});

		expect(screen.getByText('Configure a weapon before tracking')).toBeTruthy();
		expect(screen.queryByTitle('Start tracking')).toBeNull();

		screen.getByLabelText('Dismiss warning').click();
		expect(onDismissAttributionWarning).toHaveBeenCalledTimes(1);
	});

	it('does not replace the stop control during an active session', () => {
		render(OverlayStrip, {
			props: {
				data: liveData({ status: 'active' }),
				attributionWarning: 'Configure a weapon before tracking',
			},
		});
		expect(screen.queryByText('Configure a weapon before tracking')).toBeNull();
		expect(screen.getByTitle('Stop tracking')).toBeTruthy();
	});
});

describe('session facets and declared mob', () => {
	it('takes the session name while idle', () => {
		render(OverlayStrip, { props: { data: liveData({ status: 'idle', sessionName: null }) } });
		expect(screen.getByPlaceholderText('Name...')).toBeTruthy();
	});

	// The name is session-grain: editing it live could only rewrite the
	// whole session's history, so a running session offers no control at
	// all (not even a clear), and the record is where it gets corrected.
	it('offers no name control at all during an active session', () => {
		render(OverlayStrip, {
			props: { data: liveData({ status: 'active', sessionName: 'ARIS Dailies' }) },
		});
		expect(screen.getByText('ARIS Dailies')).toBeTruthy();
		expect(screen.queryByPlaceholderText('Name...')).toBeNull();
		expect(screen.queryByLabelText('Clear session name')).toBeNull();
	});

	it('shows the set session name with a clear control instead of the input', () => {
		const onClearName = vi.fn();
		render(OverlayStrip, {
			props: { data: liveData({ sessionName: 'ARIS Dailies' }), onClearName },
		});

		expect(screen.getByText('ARIS Dailies')).toBeTruthy();
		expect(screen.queryByPlaceholderText('Name...')).toBeNull();

		screen.getByLabelText('Clear session name').click();
		expect(onClearName).toHaveBeenCalledTimes(1);
	});

	it('edits the skill boost while idle', () => {
		const onBoostCommit = vi.fn();
		render(OverlayStrip, {
			props: { data: liveData({ status: 'idle' }), boostDraft: '50', onBoostCommit },
		});
		const input = screen.getByLabelText('Skill boost percent') as HTMLInputElement;
		expect(input.value).toBe('50');
	});

	// The boost's grain is finer than the session (it stamps each skill
	// gain), so a pill expiring mid-session is a recordable change and the
	// control stays live throughout.
	it('keeps the boost editable during an active session', () => {
		const onBoostCommit = vi.fn();
		render(OverlayStrip, {
			props: {
				data: liveData({ status: 'active', skillBoostPercent: 50 }),
				boostDraft: '50',
				onBoostCommit,
			},
		});
		const input = screen.getByLabelText('Skill boost percent') as HTMLInputElement;
		expect(input.value).toBe('50');
		expect(input.disabled).toBe(false);
	});

	it('offers the quest picker only while a session is active', () => {
		const questTrigger = () => screen.getByText('Pick').closest('button') as HTMLButtonElement;

		const { unmount } = render(OverlayStrip, { props: { data: liveData({ status: 'idle' }) } });
		expect(questTrigger().disabled).toBe(true);
		unmount();

		const onQuestTrigger = vi.fn();
		render(OverlayStrip, {
			props: { data: liveData({ status: 'active' }), onQuestTrigger },
		});
		expect(questTrigger().disabled).toBe(false);
		questTrigger().click();
		expect(onQuestTrigger).toHaveBeenCalledTimes(1);
	});

	it('shows the declared quest with a clear control', () => {
		const onClearQuest = vi.fn();
		render(OverlayStrip, {
			props: { data: liveData({ status: 'active' }), questLabel: 'ARIS Daily I', onClearQuest },
		});

		expect(screen.getByText('ARIS Daily I')).toBeTruthy();
		screen.getByLabelText('Clear declared quest').click();
		expect(onClearQuest).toHaveBeenCalledTimes(1);
	});

	it('shows the mob input when no mob is declared', () => {
		render(OverlayStrip, { props: { data: liveData({ currentMob: null }) } });
		expect(screen.getByPlaceholderText('Mob...')).toBeTruthy();
	});

	it('hides the release control when no mob is declared', () => {
		render(OverlayStrip, { props: { data: liveData({ currentMob: null }) } });
		expect(screen.queryByLabelText('Release mob')).toBeNull();
	});

	it('shows the declared mob with a release control instead of the input', () => {
		const onReleaseMob = vi.fn();
		render(OverlayStrip, {
			props: { data: liveData({ currentMob: 'Atrox Young' }), onReleaseMob },
		});

		expect(screen.getByText('Atrox Young')).toBeTruthy();
		expect(screen.queryByPlaceholderText('Mob...')).toBeNull();

		screen.getByLabelText('Release mob').click();
		expect(onReleaseMob).toHaveBeenCalledTimes(1);
	});

	it('offers the mob control during an active session (declarations move mid-session)', () => {
		render(OverlayStrip, {
			props: { data: liveData({ status: 'active', currentMob: null }) },
		});
		expect(screen.getByPlaceholderText('Mob...')).toBeTruthy();
	});

	it('surfaces a facet write failure beside the controls', () => {
		render(OverlayStrip, {
			props: { data: liveData(), facetError: 'Skill boost is fixed for the active session' },
		});
		expect(screen.getByText('Skill boost is fixed for the active session')).toBeTruthy();
	});

	it('surfaces the popup launch error under the input when the menu is closed', () => {
		render(OverlayStrip, {
			props: {
				data: liveData({ currentMob: null }),
				overlayMenuLaunchError: 'Popup route did not become ready',
				mobMenuOpen: false,
			},
		});
		expect(screen.getByText('Popup route did not become ready')).toBeTruthy();
	});
});

describe('derived activity feedback', () => {
	it('names the activity the held tool implies', () => {
		render(OverlayStrip, {
			props: { data: liveData({ currentTool: 'ChopChop Jr', currentActivity: 'treecutting' }) },
		});
		expect(screen.getByTestId('activity-feedback').textContent?.trim()).toBe('Tree Cutting');
	});

	it('says nothing when no tool is known', () => {
		render(OverlayStrip, {
			props: { data: liveData({ currentTool: null, currentActivity: null }) },
		});
		expect(screen.queryByTestId('activity-feedback')).toBeNull();
	});
});

describe('customisable stat pills', () => {
	it('renders only the enabled overlay stats, through the real registry render', () => {
		overlayStats.current = [
			{ id: 'net' as StatId, enabled: true },
			{ id: 'kills' as StatId, enabled: false },
		];
		const status = activeStatus({ cost: 10, returns: 12.5, kill_count: 7 });
		render(OverlayStrip, { props: { data: liveData({ status: 'active' }), status } });

		const netDef = getStatDef('net' as StatId);
		const killsDef = getStatDef('kills' as StatId);
		expect(netDef && screen.getByText(netDef.label)).toBeTruthy();
		expect(killsDef && screen.queryByText(killsDef.label)).toBeNull();
		// net = returns - cost, rendered by the registry's own formatter.
		const netRender = netDef ? netDef.render(status) : null;
		expect(netRender && screen.getByText(netRender.value)).toBeTruthy();
	});

	it('renders nothing when no overlay stat is enabled', () => {
		overlayStats.current = [{ id: 'net' as StatId, enabled: false }];
		const netDef = getStatDef('net' as StatId);
		render(OverlayStrip, { props: { data: liveData({ status: 'active' }) } });
		expect(netDef && screen.queryByText(netDef.label)).toBeNull();
	});
});

describe('trifecta selector', () => {
	const trifecta = {
		activePresetId: 'p1',
		presetName: 'Hunting Set',
		presets: [
			{ id: 'p1', name: 'Hunting Set' },
			{ id: 'p2', name: 'Mining Set' },
		],
		smallWeapon: null,
		bigWeapon: null,
		healTool: null,
	};

	it('renders the active preset name and forwards the trigger click with its anchor', () => {
		const onTrifectaTrigger = vi.fn();
		render(OverlayStrip, {
			props: {
				data: liveData({
					status: 'active',
					weaponAttribution: 'trifecta',
					trifectaAttribution: trifecta,
				}),
				onTrifectaTrigger,
			},
		});

		const trigger = screen.getByTitle('Hunting Set') as HTMLButtonElement;
		expect(trigger.getAttribute('aria-expanded')).toBe('false');
		trigger.click();
		expect(onTrifectaTrigger).toHaveBeenCalledWith(trigger);
	});

	it('reflects the open menu and saving state on the trigger', () => {
		render(OverlayStrip, {
			props: {
				data: liveData({ weaponAttribution: 'trifecta', trifectaAttribution: trifecta }),
				trifectaMenuOpen: true,
				trifectaSaving: true,
			},
		});

		const trigger = screen.getByTitle('Hunting Set') as HTMLButtonElement;
		expect(trigger.getAttribute('aria-expanded')).toBe('true');
		expect(trigger.disabled).toBe(true);
	});

	it('surfaces the trifecta error under the trigger', () => {
		render(OverlayStrip, {
			props: {
				data: liveData({ weaponAttribution: 'trifecta', trifectaAttribution: trifecta }),
				trifectaError: 'Popup route did not become ready',
			},
		});
		expect(screen.getByText('Popup route did not become ready')).toBeTruthy();
	});

	it('falls back to the current tool readout under hotbar attribution', () => {
		render(OverlayStrip, {
			props: {
				data: liveData({ weaponAttribution: 'hotbar', currentTool: 'Sollomate Opalo' }),
			},
		});
		expect(screen.getByText('Sollomate Opalo')).toBeTruthy();
		expect(screen.queryByTitle('Hunting Set')).toBeNull();
	});

	it('shows the guardrail alert in place of the tool readout on a mismatch', () => {
		render(OverlayStrip, {
			props: {
				data: liveData({
					status: 'active',
					weaponAttribution: 'hotbar',
					currentTool: 'Terratech PH-4 (L)',
					harvestGuardrail: {
						expectedTool: 'Terratech PH-1 (L)',
						observedTool: 'Terratech PH-4 (L)',
						treeSize: 'short',
						atEpoch: 1_784_600_000,
					},
				}),
			},
		});
		const alert = screen.getByTestId('guardrail-alert');
		// The believed tool shows in red; the corrected attribution beneath.
		const believed = screen.getByText('Terratech PH-4 (L)');
		expect(believed.className).toContain('text-red-400');
		const recording = screen.getByText('Recording: Terratech PH-1 (L)');
		expect(recording.className).toContain('text-white/70');
		// The recorded tool must stay readable in full: no truncation.
		expect(recording.className).toContain('whitespace-nowrap');
		expect(recording.className).not.toContain('truncate');
		expect(alert.title).toBe(
			'Board output says Terratech PH-1 (L); hotbar shows Terratech PH-4 (L)',
		);
	});

	it('names the no-tool case in the guardrail alert', () => {
		render(OverlayStrip, {
			props: {
				data: liveData({
					status: 'active',
					weaponAttribution: 'hotbar',
					harvestGuardrail: {
						expectedTool: 'Terratech PH-1 (L)',
						observedTool: null,
						treeSize: 'short',
						atEpoch: 1_784_600_000,
					},
				}),
			},
		});
		expect(screen.getByText('No tool').className).toContain('text-red-400');
		expect(screen.getByTestId('guardrail-alert').title).toBe(
			'Board output says Terratech PH-1 (L); hotbar shows no tool',
		);
	});
});

describe('armour cost control', () => {
	it('is disabled without a session id', () => {
		render(OverlayStrip, { props: { data: liveData() } });
		const button = screen.getByText('Cost') as HTMLButtonElement;
		expect(button.disabled).toBe(true);
	});

	it('toggles through the callback when a session id exists', () => {
		const onArmourCostToggle = vi.fn();
		render(OverlayStrip, {
			props: { data: liveData({ status: 'active' }), armourSessionId: 's1', onArmourCostToggle },
		});
		const button = screen.getByText('Cost') as HTMLButtonElement;
		expect(button.disabled).toBe(false);
		button.click();
		expect(onArmourCostToggle).toHaveBeenCalledTimes(1);
	});

	it('surfaces the armour cost error while the popup is closed', () => {
		render(OverlayStrip, {
			props: {
				data: liveData({ status: 'active' }),
				armourSessionId: 's1',
				armourCostError: 'Armour cost popup did not become ready',
				armourCostOpen: false,
			},
		});
		expect(screen.getByText('Armour cost popup did not become ready')).toBeTruthy();
	});
});

describe('post-session bar', () => {
	const postSession = {
		data: liveData({ status: 'idle' }),
		lastSessionId: 's1',
	};

	it('replaces the active strip once a session has ended', () => {
		render(OverlayStrip, { props: postSession });
		expect(screen.getByText('Session ended')).toBeTruthy();
		expect(screen.queryByTitle('Start tracking')).toBeNull();
	});

	it('does not appear while idle with no finished session', () => {
		render(OverlayStrip, { props: { data: liveData() } });
		expect(screen.queryByText('Session ended')).toBeNull();
		expect(screen.getByTitle('Start tracking')).toBeTruthy();
	});

	it('renders the last-session cost and signed net', () => {
		render(OverlayStrip, {
			props: {
				...postSession,
				lastSessionStats: { cost: 25.5, returns: 27.75, pes: 1.2, net: 2.25 },
			},
		});
		expect(screen.getByText('25.50')).toBeTruthy();
		expect(screen.getByText('+2.25')).toBeTruthy();
	});

	it('renders a negative net without the plus sign', () => {
		render(OverlayStrip, {
			props: {
				...postSession,
				lastSessionStats: { cost: 25.5, returns: 20, pes: 1.2, net: -5.5 },
			},
		});
		expect(screen.getByText('-5.50')).toBeTruthy();
	});

	it('offers the quest-link suggestion and forwards the decision', () => {
		const onQuestLinkDecision = vi.fn();
		render(OverlayStrip, {
			props: {
				...postSession,
				questLinkSuggestion: {
					sessionId: 's1',
					suggestionType: 'quest',
					reason: 'single_quest',
					questId: 'q1',
					questName: 'Iron Challenge',
					playlistId: null,
					playlistName: null,
				},
				onQuestLinkDecision,
			},
		});

		expect(screen.getByText('Iron Challenge')).toBeTruthy();
		screen.getByText('Yes').click();
		expect(onQuestLinkDecision).toHaveBeenCalledWith('accept');
		screen.getByText('No').click();
		expect(onQuestLinkDecision).toHaveBeenCalledWith('decline');
	});

	it('shows the quest-link outcome message with a dismiss control', () => {
		const onDismissQuestLinkMessage = vi.fn();
		render(OverlayStrip, {
			props: {
				...postSession,
				questLinkMessage: 'Linked to Iron Challenge',
				onDismissQuestLinkMessage,
			},
		});

		expect(screen.getByText('Linked to Iron Challenge')).toBeTruthy();
		screen.getByText('Done').click();
		expect(onDismissQuestLinkMessage).toHaveBeenCalledTimes(1);
	});
});
