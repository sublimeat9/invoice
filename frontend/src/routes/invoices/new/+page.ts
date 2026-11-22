import { customers, products } from '#lib/api.ts';
import type { PageLoad } from './$types';

// Catalogs only feed the autocomplete: if they fail, the form still works.
export const load: PageLoad = async () => {
	const [p, c] = await Promise.allSettled([products.list({ limit: 200 }), customers.list({ limit: 200 })]);
	return {
		products: p.status === 'fulfilled' ? p.value.items : [],
		customers: c.status === 'fulfilled' ? c.value.items : []
	};
};
