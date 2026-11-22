import { ApiError, obtener } from '#lib/server/api.ts';
import { error } from '@sveltejs/kit';
import type { RequestHandler } from './$types';

export const GET: RequestHandler = async ({ params }) => {
	try {
		const f = await obtener(params.clave);
		return new Response(f.xml_firmado ?? '', {
			headers: {
				'content-type': 'application/xml; charset=utf-8',
				'content-disposition': `attachment; filename="${f.clave_acceso}.xml"`
			}
		});
	} catch (e) {
		if (e instanceof ApiError) error(e.status === 404 ? 404 : 502, e.message);
		throw e;
	}
};
