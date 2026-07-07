import { describe, expect, it } from 'vitest';

import { ApiError } from './client';

describe('ApiError', () => {
	it('carries kind, message, and a distinguishing name', () => {
		const err = new ApiError('conflict', 'teapot');
		expect(err).toBeInstanceOf(Error);
		expect(err.kind).toBe('conflict');
		expect(err.message).toBe('teapot');
		expect(err.name).toBe('ApiError');
	});
});
