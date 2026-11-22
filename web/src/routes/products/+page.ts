import { errorMessage, products } from '#lib/api.ts';
import type { PageLoad } from './$types';

const PAGE_SIZE = 20;

export const load: PageLoad = async ({ url }) => {
	const q = url.searchParams.get('q')?.trim() ?? '';
	const page = Math.max(1, Number(url.searchParams.get('page')) || 1);
	try {
		const r = await products.list({ q, limit: PAGE_SIZE, page });
		return { items: r.items, total: r.total, pages: Math.max(1, Math.ceil(r.total / PAGE_SIZE)), page, q, error: null as string | null };
	} catch (e) {
		return { items: [], total: 0, pages: 1, page, q, error: errorMessage(e) };
	}
};
