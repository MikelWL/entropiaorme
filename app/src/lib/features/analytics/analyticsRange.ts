export const ANALYTICS_RANGES = ['All Time', '30d', '90d', '1y'] as const;

export type AnalyticsRange = (typeof ANALYTICS_RANGES)[number];
export type AnalyticsPeriod = 'all' | '30d' | '90d' | '1y';

const PERIOD_BY_RANGE: Record<AnalyticsRange, AnalyticsPeriod> = {
	'All Time': 'all',
	'30d': '30d',
	'90d': '90d',
	'1y': '1y',
};

export function analyticsPeriod(range: AnalyticsRange): AnalyticsPeriod {
	return PERIOD_BY_RANGE[range];
}

export function isAnalyticsRange(value: string): value is AnalyticsRange {
	return ANALYTICS_RANGES.some((range) => range === value);
}
