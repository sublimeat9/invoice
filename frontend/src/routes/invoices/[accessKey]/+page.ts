import { errorMessage, getInvoice } from '#lib/api.ts';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params }) => {
	try {
		return { invoice: await getInvoice(params.accessKey), error: null as string | null };
	} catch (e) {
		return { invoice: null, error: errorMessage(e) };
	}
};
