import { getConfig, readSession, type Environment } from '#lib/api.ts';
import { redirect } from '@sveltejs/kit';
import type { LayoutLoad } from './$types';

// Single-page app: the same build runs on the web and packaged inside Tauri.
export const ssr = false;
export const prerender = false;

export const load: LayoutLoad = async ({ url }) => {
	// Everything except the login page needs a session.
	if (!readSession()) {
		if (url.pathname !== '/login') redirect(307, '/login');
		return { configReady: null as boolean | null, environment: null as Environment | null };
	}
	try {
		const c = await getConfig();
		return { configReady: c.ready as boolean | null, environment: c.environment as Environment | null };
	} catch {
		// If the backend does not answer, each page shows its own error.
		return { configReady: null as boolean | null, environment: null as Environment | null };
	}
};
