import { errorMessage, searchInvoices, type InvoiceSearch } from '#lib/api.ts';
import type { PageLoad } from './$types';

const PAGE_SIZE = 20;
const FIELDS = ['date_from', 'date_to', 'id_number', 'status', 'access_key'] as const;

/** Reads the filters from the URL (single source of truth: links are shareable). */
export function filtersFrom(url: URL): InvoiceSearch {
	const filters: InvoiceSearch = {};
	for (const k of FIELDS) {
		const v = url.searchParams.get(k)?.trim();
		if (v) filters[k] = v;
	}
	return filters;
}

export const load: PageLoad = async ({ url }) => {
	const filters = filtersFrom(url);
	const page = Math.max(1, Number(url.searchParams.get('page')) || 1);
	try {
		const r = await searchInvoices({ ...filters, limit: PAGE_SIZE, page });
		return { invoices: r.items, total: r.total, pages: Math.max(1, Math.ceil(r.total / PAGE_SIZE)), page, filters, error: null as string | null };
	} catch (e) {
		return { invoices: [], total: 0, pages: 1, page, filters, error: errorMessage(e) };
	}
};
