import { errorMessage, getConfig } from '#lib/api.ts';
import type { PageLoad } from './$types';

export const load: PageLoad = async () => {
	try {
		return { config: await getConfig(), error: null as string | null };
	} catch (e) {
		return { config: null, error: errorMessage(e) };
	}
};
