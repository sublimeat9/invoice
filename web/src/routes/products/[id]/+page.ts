import { errorMessage, products } from '#lib/api.ts';
import type { PageLoad } from './$types';

/** `/products/new` creates; `/products/<id>` edits. */
export const load: PageLoad = async ({ params }) => {
	if (params.id === 'new') return { product: null, error: null as string | null };
	try {
		return { product: await products.get(Number(params.id)), error: null as string | null };
	} catch (e) {
		return { product: null, error: errorMessage(e) };
	}
};
