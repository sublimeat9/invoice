import { ApiError, listar, reintentar } from '#lib/server/api.ts';
import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';

export const load: PageServerLoad = async () => {
	try {
		return { facturas: await listar(), error: null as string | null };
	} catch (e) {
		return { facturas: [], error: e instanceof ApiError ? e.message : 'Error inesperado' };
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
