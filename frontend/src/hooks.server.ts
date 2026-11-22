import { API_TOKEN, BACKEND_URL } from '$app/env/private';
import type { Handle } from '@sveltejs/kit/hooks';

const METHODS_WITH_BODY = new Set(['POST', 'PUT', 'PATCH']);

/**
 * Web mode: the browser talks to `/api/*` and this hook forwards the call to the backend adding
 * the Bearer token, so the token never reaches the browser.
 */
export const handle: Handle = async ({ event, resolve }) => {
	if (!event.url.pathname.startsWith('/api/')) return resolve(event);

	if (API_TOKEN.length < 16) {
		return new Response(JSON.stringify({ error: 'API_TOKEN is not configured on the server' }), {
			status: 500,
			headers: { 'content-type': 'application/json' }
		});
	}

	const path = event.url.pathname.slice('/api'.length) + event.url.search;
	const headers = new Headers({ authorization: `Bearer ${API_TOKEN}` });
	const contentType = event.request.headers.get('content-type');
	if (contentType) headers.set('content-type', contentType);

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
