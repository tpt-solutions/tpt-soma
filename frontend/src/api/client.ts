import createClient from 'openapi-fetch';
import type { paths } from './schema';

/** Typed client for the tpt-soma API. Types come from docs/api/openapi.json via `npm run gen:api`. */
export function createApiClient(baseUrl: string, token: string, fetchImpl?: typeof fetch) {
  return createClient<paths>({
    baseUrl,
    fetch: fetchImpl,
    headers: { Authorization: `Bearer ${token}` },
  });
}

/** Returns the data from a client call, or throws `status: body` like the old fetch helper did. */
export function unwrap<T>(result: { data?: T; error?: unknown; response: Response }): T {
  if (result.error !== undefined) {
    const body = typeof result.error === 'string' ? result.error : JSON.stringify(result.error);
    throw new Error(`${result.response.status}: ${body}`);
  }
  return result.data as T;
}
