import { ApiError, buscar, reintentar, type Busqueda } from '#lib/server/api.ts';
import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';

const POR_PAGINA = 20;
const CAMPOS = ['desde', 'hasta', 'identificacion', 'estado', 'clave'] as const;

/** Lee los filtros de la URL (una sola fuente de verdad: así se puede compartir el enlace). */
export function filtrosDe(url: URL): Busqueda {
	const f: Busqueda = {};
	for (const k of CAMPOS) {
		const v = url.searchParams.get(k)?.trim();
		if (v) f[k] = v;
	}
	return f;
}

export const load: PageServerLoad = async ({ url }) => {
	const filtros = filtrosDe(url);
	const pagina = Math.max(1, Number(url.searchParams.get('pagina')) || 1);
	try {
		const r = await buscar({ ...filtros, limite: POR_PAGINA, pagina });
		return {
			facturas: r.items,
			total: r.total,
			paginas: Math.max(1, Math.ceil(r.total / POR_PAGINA)),
			pagina,
			filtros,
			error: null as string | null
		};
	} catch (e) {
		return {
			facturas: [],
			total: 0,
			paginas: 1,
			pagina,
			filtros,
			error: e instanceof ApiError ? e.message : 'Error inesperado'
		};
	}
};

export const actions: Actions = {
	reintentar: async () => {
		try {
			return { reintentadas: (await reintentar()).length };
		} catch (e) {
			return fail(502, { error: e instanceof ApiError ? e.message : 'Error inesperado' });
		}
	}
};
