import { API_TOKEN, BACKEND_URL } from '$app/env/private';

export type Registro = {
	clave_acceso: string;
	numero: string;
	secuencial: number;
	fecha_emision: string | null;
	tipo_comprobante: string;
	identificacion: string | null;
	razon_social: string | null;
	/** En centavos. */
	valor_total: number | null;
	estado: string;
	numero_autorizacion: string | null;
	fecha_autorizacion: string | null;
	mensajes: string | null;
	xml_firmado?: string;
};

export type Linea = {
	codigo_principal: string;
	descripcion: string;
	cantidad: number;
	precio_unitario_centavos: number;
	descuento_centavos: number;
	iva: '15' | '5' | '0' | 'no_objeto' | 'exento';
};

export type NuevaFactura = {
	comprador?: {
		tipo_identificacion: string;
		razon_social: string;
		identificacion: string;
		direccion?: string;
	};
	lineas: Linea[];
	forma_pago: string;
	info_adicional: [string, string][];
};

export class ApiError extends Error {
	constructor(
		public status: number,
		message: string
	) {
		super(message);
	}
}

/** Llama al backend desde el servidor de SvelteKit; el token nunca llega al navegador. */
async function llamar<T>(path: string, init: RequestInit = {}): Promise<{ status: number; data: T }> {
	const token = API_TOKEN;
	const base = BACKEND_URL;
	let res: Response;
	try {
		res = await fetch(base + path, {
			...init,
			headers: {
				...(init.body ? { 'content-type': 'application/json' } : {}),
				authorization: `Bearer ${token}`
			}
		});
	} catch {
		throw new ApiError(503, 'No se pudo conectar con el backend');
	}
	const data = await res.json().catch(() => null);
	if (!res.ok) throw new ApiError(res.status, data?.error ?? `Error ${res.status} del backend`);
	return { status: res.status, data: data as T };
}

export type Busqueda = {
	desde?: string;
	hasta?: string;
	identificacion?: string;
	tipo?: string;
	estado?: string;
	clave?: string;
	limite?: number;
	pagina?: number;
};

function query(b: Busqueda): string {
	const p = new URLSearchParams();
	for (const [k, v] of Object.entries(b)) if (v !== undefined && v !== '') p.set(k, String(v));
	const s = p.toString();
	return s ? `?${s}` : '';
}

export const buscar = (b: Busqueda) =>
	llamar<{ total: number; items: Registro[] }>(`/facturas${query(b)}`).then((r) => r.data);

/** Reporte CSV con los mismos filtros (el backend limita a 10 000 filas). */
export async function reporteCsv(b: Busqueda): Promise<string> {
	const res = await fetch(`${BACKEND_URL}/facturas/reporte.csv${query(b)}`, {
		headers: { authorization: `Bearer ${API_TOKEN}` }
	}).catch(() => {
		throw new ApiError(503, 'No se pudo conectar con el backend');
	});
	if (!res.ok) throw new ApiError(res.status, `Error ${res.status} del backend`);
	return res.text();
}

export const obtener = (clave: string) =>
	llamar<Registro>(`/facturas/${encodeURIComponent(clave)}`).then((r) => r.data);

export const emitir = (f: NuevaFactura) =>
	llamar<Registro>('/facturas', { method: 'POST', body: JSON.stringify(f) }).then((r) => r.data);

export const reintentar = () =>
	llamar<{ clave_acceso: string; estado?: string; error?: string }[]>('/facturas/reintentar', {
		method: 'POST'
	}).then((r) => r.data);
