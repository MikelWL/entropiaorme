import { describe, expect, it } from 'vitest';
import { ApiError } from '$lib/api/client';
import { describeError } from './errorState';

describe('describeError', () => {
	const cases: [name: string, input: unknown, expected: string][] = [
		['an Error surfaces its message', new Error('database locked'), 'database locked'],
		[
			'an ApiError surfaces its message',
			new ApiError('notFound', 'quest not found'),
			'quest not found',
		],
		['a non-empty string stands as-is', 'plain failure text', 'plain failure text'],
		['an Error with an empty message falls back', new Error(''), 'Something went wrong'],
		['an Error with a whitespace message falls back', new Error('   '), 'Something went wrong'],
		['an empty string falls back', '', 'Something went wrong'],
		['a whitespace string falls back', '  \n ', 'Something went wrong'],
		['null falls back', null, 'Something went wrong'],
		['undefined falls back', undefined, 'Something went wrong'],
		['a number falls back', 42, 'Something went wrong'],
		['a plain object falls back', { message: 'not an Error' }, 'Something went wrong'],
	];

	for (const [name, input, expected] of cases) {
		it(name, () => {
			expect(describeError(input)).toBe(expected);
		});
	}

	it('honours a custom fallback', () => {
		expect(describeError(null, 'Failed to load quests')).toBe('Failed to load quests');
	});

	it('prefers the message over a custom fallback when one exists', () => {
		expect(describeError(new Error('boom'), 'Failed to load quests')).toBe('boom');
	});
});
