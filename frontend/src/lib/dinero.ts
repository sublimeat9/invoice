/** Convierte "15", "15.5" o "15,50" a centavos enteros sin pasar por punto flotante. */
export function aCentavos(texto: string): number | null {
	const m = texto.trim().replace(',', '.').match(/^(\d{1,9})(?:\.(\d{1,2}))?$/);
	if (!m) return null;
	return Number(m[1]) * 100 + Number((m[2] ?? '').padEnd(2, '0') || 0);
}

export function formatear(centavos: number): string {
	return `$${(centavos / 100).toFixed(2)}`;
}

export const TARIFA: Record<string, number> = { '15': 15, '5': 5, '0': 0, no_objeto: 0, exento: 0 };

/** Mismo cálculo que el backend: IVA por línea con redondeo half-up a centavos. */
export function totales(
	lineas: { cantidad: number; precio: number; descuento: number; iva: string }[]
): { subtotal: number; iva: number; total: number } {
	let subtotal = 0;
	let iva = 0;
	for (const l of lineas) {
		const base = l.cantidad * l.precio - l.descuento;
		subtotal += base;
		iva += Math.floor((base * (TARIFA[l.iva] ?? 0) + 50) / 100);
	}
	return { subtotal, iva, total: subtotal + iva };
}
