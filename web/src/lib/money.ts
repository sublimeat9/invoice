/** Converts "15", "15.5" or "15,50" to integer cents without going through floating point. */
export function toCents(text: string): number | null {
	const m = text.trim().replace(',', '.').match(/^(\d{1,9})(?:\.(\d{1,2}))?$/);
	if (!m) return null;
	return Number(m[1]) * 100 + Number((m[2] ?? '').padEnd(2, '0') || 0);
}

export function formatMoney(cents: number): string {
	return `$${(cents / 100).toFixed(2)}`;
}

/** 1500 -> "15.00" (to fill text inputs). */
export function centsToText(cents: number): string {
	return (cents / 100).toFixed(2);
}

const VAT_RATE: Record<string, number> = { '15': 15, '5': 5, '0': 0, not_subject: 0, exempt: 0 };

/** Same calculation as the backend: VAT per line, rounded half-up to cents. */
export function totals(
	lines: { quantity: number; price: number; discount: number; vat: string }[]
): { subtotal: number; vat: number; total: number } {
	let subtotal = 0;
	let vat = 0;
	for (const l of lines) {
		const base = l.quantity * l.price - l.discount;
		subtotal += base;
		vat += Math.floor((base * (VAT_RATE[l.vat] ?? 0) + 50) / 100);
	}
	return { subtotal, vat, total: subtotal + vat };
}
