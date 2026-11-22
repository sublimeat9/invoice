import { errorMessage, users } from '#lib/api.ts';
import type { PageLoad } from './$types';

export const load: PageLoad = async () => {
	try {
		return { items: await users.list(), error: null as string | null };
	} catch (e) {
		return { items: [], error: errorMessage(e) };
	}
};
