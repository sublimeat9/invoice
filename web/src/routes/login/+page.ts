import { authStatus, readSession } from '#lib/api.ts';
import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = async () => {
	if (readSession()) redirect(307, '/');
	try {
		return { setupRequired: (await authStatus()).setup_required, unreachable: false };
	} catch {
		return { setupRequired: false, unreachable: true };
	}
};
