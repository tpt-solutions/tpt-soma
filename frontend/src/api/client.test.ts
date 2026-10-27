import { describe, expect, it } from 'vitest';
import { createApiClient } from './client';

describe('createApiClient', () => {
  it('sends the bearer token and the cohort id in the path', async () => {
    let seen: Request | undefined;
    const fakeFetch: typeof fetch = async (input) => {
      seen = input instanceof Request ? input : new Request(input);
      return new Response(JSON.stringify({ cohort_id: 'cohort-a', epsilon_cap: 1, remaining_budget: 1, requested_sensitivity: 1, would_succeed: true }), {
        headers: { 'Content-Type': 'application/json' },
      });
    };

    const client = createApiClient('http://api.test', 'tok-123', fakeFetch);
    const { data, error } = await client.GET('/api/v1/cohorts/{cohort_id}/budget', {
      params: { path: { cohort_id: 'cohort-a' }, query: { sensitivity: 0.5 } },
    });

    expect(error).toBeUndefined();
    expect(data?.would_succeed).toBe(true);
    expect(seen?.headers.get('Authorization')).toBe('Bearer tok-123');
    expect(seen?.url).toBe('http://api.test/api/v1/cohorts/cohort-a/budget?sensitivity=0.5');
  });
});
