import { customers, errorMessage } from '#lib/api.ts';
import type { PageLoad } from './$types';

/** `/customers/new` creates; `/customers/<id>` edits. */
export const load: PageLoad = async ({ params }) => {
	if (params.id === 'new') return { customer: null, error: null as string | null };
	try {
		return { customer: await customers.get(Number(params.id)), error: null as string | null };
	} catch (e) {
		return { customer: null, error: errorMessage(e) };
	}
};
