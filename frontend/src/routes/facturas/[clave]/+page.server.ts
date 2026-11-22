import { ApiError, obtener } from '#lib/server/api.ts';
import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ params }) => {
	try {
		const f = await obtener(params.clave);
		// El XML se descarga por su propia ruta; no se manda a la página.
		const { xml_firmado: _xml, ...registro } = f;
		return { factura: registro };
	} catch (e) {
		if (e instanceof ApiError) error(e.status === 404 ? 404 : 502, e.message);
		throw e;
	}
};
