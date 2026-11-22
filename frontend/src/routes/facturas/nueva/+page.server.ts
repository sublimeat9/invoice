import { aCentavos } from '#lib/dinero.ts';
import { ApiError, emitir, type Linea } from '#lib/server/api.ts';
import { fail, redirect } from '@sveltejs/kit';
import type { Actions } from './$types';

type LineaForm = { codigo: string; descripcion: string; cantidad: string; precio: string; descuento: string; iva: string };
const IVAS = ['15', '5', '0', 'no_objeto', 'exento'];

export const actions: Actions = {
	default: async ({ request }) => {
		const f = await request.formData();
		const texto = (k: string) => String(f.get(k) ?? '').trim();
		const valores = Object.fromEntries([...f.entries()].map(([k, v]) => [k, String(v)]));

		let crudas: LineaForm[];
		try {
			crudas = JSON.parse(texto('lineas'));
			if (!Array.isArray(crudas)) throw new Error();
		} catch {
			return fail(400, { error: 'Líneas inválidas', valores });
		}
		if (crudas.length === 0) return fail(400, { error: 'Agregue al menos una línea', valores });

		const lineas: Linea[] = [];
		for (const [i, l] of crudas.entries()) {
			const n = i + 1;
			const cantidad = Number(l.cantidad);
			const precio = aCentavos(String(l.precio));
			const descuento = aCentavos(String(l.descuento || '0'));
			if (!String(l.descripcion).trim()) return fail(400, { error: `Línea ${n}: falta la descripción`, valores });
			if (!Number.isInteger(cantidad) || cantidad < 1)
				return fail(400, { error: `Línea ${n}: la cantidad debe ser un entero mayor a 0`, valores });
			if (precio === null || precio <= 0)
				return fail(400, { error: `Línea ${n}: precio inválido (use hasta 2 decimales)`, valores });
			if (descuento === null || descuento > cantidad * precio)
				return fail(400, { error: `Línea ${n}: descuento inválido`, valores });
			if (!IVAS.includes(l.iva)) return fail(400, { error: `Línea ${n}: IVA inválido`, valores });
			lineas.push({
				codigo_principal: String(l.codigo).trim() || 'S/C',
				descripcion: String(l.descripcion).trim(),
				cantidad,
				precio_unitario_centavos: precio,
				descuento_centavos: descuento,
				iva: l.iva as Linea['iva']
			});
		}

		let comprador;
		if (!f.get('consumidor_final')) {
			const identificacion = texto('identificacion');
			const tipo = texto('tipo_identificacion');
			if (!texto('razon_social') || !identificacion)
				return fail(400, { error: 'Complete el nombre y la identificación del comprador', valores });
			if (tipo === '05' && !/^\d{10}$/.test(identificacion))
				return fail(400, { error: 'La cédula debe tener 10 dígitos', valores });
			if (tipo === '04' && !/^\d{13}$/.test(identificacion))
				return fail(400, { error: 'El RUC debe tener 13 dígitos', valores });
			comprador = {
				tipo_identificacion: tipo,
				razon_social: texto('razon_social'),
				identificacion,
				direccion: texto('direccion') || undefined
			};
		}

		const email = texto('email');
		if (email && !/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(email))
			return fail(400, { error: 'Correo electrónico inválido', valores });

		let registro;
		try {
			registro = await emitir({
				comprador,
				lineas,
				forma_pago: texto('forma_pago') || '01',
				info_adicional: email ? [['Email', email]] : []
			});
		} catch (e) {
			return fail(e instanceof ApiError && e.status < 500 ? e.status : 502, {
				error: e instanceof ApiError ? e.message : 'Error inesperado',
				valores
			});
		}
		redirect(303, `/facturas/${registro.clave_acceso}`);
	}
};
