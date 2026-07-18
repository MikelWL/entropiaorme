<script lang="ts">
	import '../app.css';
	import favicon from '$lib/assets/favicon.png';
	import { Sidebar, Titlebar, UpdateToast } from '$lib/components';
	import GuideOverlay from '$lib/guide/GuideOverlay.svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import { getOnboardingComplete } from '$lib/onboarding';
	import { isTosAccepted } from '$lib/tos';
	import { theme, initTheme } from '$lib/theme.svelte';
	import { initStatsCustomisation } from '$lib/statsCustomisation.svelte';
	import { initCartographyOverlay } from '$lib/features/maps/cartographyOverlay.svelte';
	import { initActivityArchive } from '$lib/activityArchive.svelte';
	import { initNews, newsOptIn, newsHasUnread, NEWS_PREFERENCE_KEYS } from '$lib/news.svelte';
	import { initUpdater, maybeCheckOnLaunch, updateAvailable } from '$lib/updater.svelte';
	import { maybeRefreshOnMount } from '$lib/newsFetch';
	import { initMarketData, MARKET_DATA_PREFERENCE_KEYS } from '$lib/marketData.svelte';
	import { maybeRefreshMarketSnapshotOnMount } from '$lib/marketDataFetch';
	import { getPreference } from '$lib/preferences';
	import { startEventRelay } from '$lib/realtime/eventRelay';
	import {
		NavDashboard,
		NavAnalytics,
		NavCharacter,
		NavQuests,
		NavEquipment,
		NavMaps,
		NavMarket,
		NavSettings,
		NavNews,
		NavUpdates,
	} from '$lib/icons';

	let { children } = $props();

	// Open the backend event stream once for the app. Guarded to the main
	// window inside the relay; the returned stopper runs on window teardown.
	onMount(() => startEventRelay());

	let onboardingChecked = $state(false);
	let welcomingIn = $state(false);

	$effect(() => {
		if (typeof document !== 'undefined') {
			document.documentElement.setAttribute('data-theme', theme.current);
		}
	});

	onMount(async () => {
		await Promise.all([
			initTheme(),
			initStatsCustomisation(),
			initCartographyOverlay(),
			initActivityArchive(),
			initNews(),
			initUpdater(),
			initMarketData(),
		]);
		void maybeRefreshOnMount();
		void maybeRefreshMarketSnapshotOnMount();
		if (
			typeof sessionStorage !== 'undefined' &&
			sessionStorage.getItem('welcome_just_finished') === '1'
		) {
			sessionStorage.removeItem('welcome_just_finished');
			welcomingIn = true;
		}
		const complete = await getOnboardingComplete();
		const path = page.url.pathname;
		const isWelcome = path.startsWith('/welcome');
		const isOverlay =
			path.startsWith('/overlay') ||
			path.startsWith('/scan-overlay') ||
			path.startsWith('/cartography-overlay');
		if (!isWelcome && !isOverlay) {
			if (!complete) {
				await goto('/welcome', { replaceState: true });
			} else if (!(await isTosAccepted())) {
				await goto('/welcome/terms', { replaceState: true });
			} else if (
				!(await getPreference<boolean>(NEWS_PREFERENCE_KEYS.optInSeen, false)) ||
				!(await getPreference<boolean>(MARKET_DATA_PREFERENCE_KEYS.optInSeen, false))
			) {
				// The networking-consent reprompt: shown until every online
				// feature's choice has been seen, so features added after a
				// user onboarded still get an explicit yes/no.
				await goto('/welcome/news-opt-in', { replaceState: true });
			} else {
				// Fully onboarded and past the networking-consent step: it is now
				// safe to make the launch-time update check (gated on the opt-out
				// preference). Never fires before the user has passed that step.
				void maybeCheckOnLaunch();
			}
		}
		onboardingChecked = true;
	});

	const navItems = [
		{ id: '/', label: 'Dashboard', icon: NavDashboard },
		{ id: '/analytics', label: 'Analytics', icon: NavAnalytics },
		{ id: '/character', label: 'Character', icon: NavCharacter },
		{ id: '/quests', label: 'Quests', icon: NavQuests },
		{ id: '/equipment', label: 'Equipment', icon: NavEquipment },
		{ id: '/market', label: 'Market', icon: NavMarket },
		{ id: '/maps', label: 'Maps', icon: NavMaps },
	];

	let footerNavItems = $derived([
		...(newsOptIn.current
			? [
					{
						id: '/news',
						label: 'News & Updates',
						icon: NavNews,
						...(newsHasUnread.current ? { indicator: 'unread' as const } : {}),
					},
				]
			: []),
		// The app-update surface surfaces in the rail only when an update is
		// pending (a quiet call-to-action with the unread dot); otherwise it is
		// reached from the toast or Settings. Keeps it distinct from the news
		// feed's "News & Updates" entry.
		...(updateAvailable.current
			? [{ id: '/updates', label: 'Updates', icon: NavUpdates, indicator: 'unread' as const }]
			: []),
	]);

	const settingsItem = { id: '/settings', label: 'Settings', icon: NavSettings };

	// Determine active page from current route
	let activePage = $derived(
		page.url.pathname === '/' ? '/' : '/' + page.url.pathname.split('/')[1]
	);

	function handleNavigate(id: string) {
		if (id !== page.url.pathname) {
			goto(id);
		}
	}
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
</svelte:head>

{#if page.url.pathname.startsWith('/overlay') || page.url.pathname.startsWith('/scan-overlay') || page.url.pathname.startsWith('/cartography-overlay')}
	{@render children()}
{:else if page.url.pathname.startsWith('/welcome')}
	<div class="flex flex-col h-screen bg-base">
		<Titlebar />
		<div class="flex-1 overflow-hidden">
			{@render children()}
		</div>
	</div>
{:else if onboardingChecked}
	<div class="flex h-screen bg-base overflow-hidden" class:welcoming-in={welcomingIn}>
		<Sidebar
			items={navItems}
			active={activePage}
			onnavigate={handleNavigate}
			footerItems={footerNavItems}
			{settingsItem}
		/>
		<div class="flex flex-col flex-1 overflow-hidden">
			<Titlebar />
			<main class="flex-1 min-h-0 overflow-y-auto">
				{@render children()}
			</main>
		</div>
	</div>
	<GuideOverlay />
	<UpdateToast />
{/if}

<style>
	.welcoming-in {
		animation: welcoming-in 520ms var(--ease-out) both;
	}
	@keyframes welcoming-in {
		from {
			opacity: 0;
			transform: scale(0.985);
		}
		to {
			opacity: 1;
			transform: scale(1);
		}
	}
</style>
