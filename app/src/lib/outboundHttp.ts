// The one hardened gateway for outbound non-loopback HTTP in the app.
// Every remote feature (the news feed, the market-data snapshot fetch
// and contribution) routes through this function; the CSP `connect-src`
// entry in app/src-tauri/entropia-orme/tauri.conf.json gates the same
// host allowlist at the webview boundary. HTTPS only, no credentials,
// no browser cache (callers manage their own), explicit timeout.

const REQUEST_TIMEOUT_MS = 10_000;

export type HttpsRequestInit = {
	method?: 'GET' | 'POST';
	headers?: Record<string, string>;
	body?: string;
	/** Statuses outside 2xx to hand back instead of throwing (e.g. 304). */
	acceptStatus?: readonly number[];
};

export async function httpsFetch(url: string, init: HttpsRequestInit = {}): Promise<Response> {
	if (!url.startsWith('https://')) {
		throw new Error(`refusing non-HTTPS URL: ${url}`);
	}
	const ctl = new AbortController();
	const timer = setTimeout(() => ctl.abort(), REQUEST_TIMEOUT_MS);
	try {
		const res = await fetch(url, {
			method: init.method ?? 'GET',
			headers: init.headers,
			body: init.body,
			credentials: 'omit',
			cache: 'no-store',
			signal: ctl.signal,
		});
		if (!res.ok && !(init.acceptStatus ?? []).includes(res.status)) {
			throw new Error(`HTTP ${res.status} for ${url}`);
		}
		return res;
	} finally {
		clearTimeout(timer);
	}
}
