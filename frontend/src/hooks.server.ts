import { BACKEND_URL } from '$app/env/private';
import type { Handle } from '@sveltejs/kit/hooks';

const METHODS_WITH_BODY = new Set(['POST', 'PUT', 'PATCH']);

/**
 * Web mode: the browser talks to `/api/*` and this hook forwards the call to the backend,
 * passing along the user's session token. The server holds no credentials of its own.
 */
export const handle: Handle = async ({ event, resolve }) => {
	if (!event.url.pathname.startsWith('/api/')) return resolve(event);

	const path = event.url.pathname.slice('/api'.length) + event.url.search;
	const headers = new Headers();
	for (const name of ['authorization', 'content-type']) {
		const value = event.request.headers.get(name);
		if (value) headers.set(name, value);
	}

	try {
		const upstream = await fetch(`${BACKEND_URL}${path}`, {
			method: event.request.method,
			headers,
			body: METHODS_WITH_BODY.has(event.request.method) ? await event.request.text() : undefined
		});
		return new Response(await upstream.arrayBuffer(), {
			status: upstream.status,
			headers: { 'content-type': upstream.headers.get('content-type') ?? 'application/json' }
		});
	} catch {
		return new Response(JSON.stringify({ error: 'The backend is not reachable' }), {
			status: 502,
			headers: { 'content-type': 'application/json' }
		});
	}
};
