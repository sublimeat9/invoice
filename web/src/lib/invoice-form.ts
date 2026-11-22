import { toCents } from './money.ts';
import type { InvoiceLine, NewInvoice, VatCode } from './api.ts';

export type LineForm = {
	code: string;
	description: string;
	quantity: string;
	price: string;
	discount: string;
	vat: string;
	product: string;
};

export type InvoiceForm = {
	finalConsumer: boolean;
	buyer: { idType: string; idNumber: string; name: string; address: string; email: string };
	lines: LineForm[];
	paymentMethod: string;
};

const VAT_CODES = ['15', '5', '0', 'not_subject', 'exempt'];

/** Validates the form and converts it to the API format (amounts in cents). */
export function buildInvoice(f: InvoiceForm): { error: string } | { invoice: NewInvoice } {
	if (f.lines.length === 0) return { error: 'Add at least one line' };

	const lines: InvoiceLine[] = [];
	for (const [i, l] of f.lines.entries()) {
		const n = i + 1;
		const quantity = Number(l.quantity);
		const price = toCents(l.price);
		const discount = toCents(l.discount || '0');
		if (!l.description.trim()) return { error: `Line ${n}: description is missing` };
		if (!Number.isInteger(quantity) || quantity < 1)
			return { error: `Line ${n}: quantity must be an integer greater than 0` };
		if (price === null || price <= 0) return { error: `Line ${n}: invalid price (up to 2 decimals)` };
		if (discount === null || discount > quantity * price) return { error: `Line ${n}: invalid discount` };
		if (!VAT_CODES.includes(l.vat)) return { error: `Line ${n}: invalid VAT` };
		lines.push({
			main_code: l.code.trim() || 'S/C',
			description: l.description.trim(),
			quantity,
			unit_price_cents: price,
			discount_cents: discount,
			vat: l.vat as VatCode
		});
	}

	let buyer: NewInvoice['buyer'];
	const b = f.buyer;
	if (!f.finalConsumer) {
		const idNumber = b.idNumber.trim();
		if (!b.name.trim() || !idNumber) return { error: 'Fill in the buyer name and ID number' };
		if (b.idType === '05' && !/^\d{10}$/.test(idNumber)) return { error: 'A cédula must have 10 digits' };
		if (b.idType === '04' && !/^\d{13}$/.test(idNumber)) return { error: 'A RUC must have 13 digits' };
		buyer = {
			id_type: b.idType,
			business_name: b.name.trim(),
			id_number: idNumber,
			address: b.address.trim() || undefined
		};
	}

	const email = b.email.trim();
	if (email && !/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(email)) return { error: 'Invalid email address' };

	return {
		invoice: {
			buyer,
			lines,
			payment_method: f.paymentMethod || '01',
			additional_info: email ? [['Email', email]] : []
		}
	};
}
