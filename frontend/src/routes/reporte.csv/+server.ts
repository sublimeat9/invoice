import { ApiError, reporteCsv } from '#lib/server/api.ts';
import { error } from '@sveltejs/kit';
import { filtrosDe } from '../+page.server.ts';
import type { RequestHandler } from './$types';

export const GET: RequestHandler = async ({ url }) => {
	try {
		return new Response(await reporteCsv(filtrosDe(url)), {
			headers: {
				'content-type': 'text/csv; charset=utf-8',
				'content-disposition': 'attachment; filename="comprobantes.csv"'
			}
		});
	} catch (e) {
		if (e instanceof ApiError) error(502, e.message);
		throw e;
	}
};
