import { describe, expect, it } from 'vitest';
import { normalisePinEmoji } from './pinIcons';

describe('normalisePinEmoji', () => {
	it.each([
		'😀',
		'👩🏽',
		'👨‍👩‍👧‍👦',
		'🇬🇧',
		'1️⃣',
		'🏴󠁧󠁢󠁳󠁣󠁴󠁿',
	])('accepts one complete emoji: %s', (emoji) => {
		expect(normalisePinEmoji(emoji)).toBe(emoji);
	});

	it.each([
		'Claim 😀',
		'text🙂more',
		'😀abc',
		'abc',
		'🇬',
	])('rejects values that are not one emoji: %s', (value) => {
		expect(normalisePinEmoji(value)).toBe('📍');
	});

	it('keeps legacy icon ids compatible', () => {
		expect(normalisePinEmoji('teleporter')).toBe('🌀');
	});
});
