<script lang="ts">
	import { goto } from '$app/navigation';
	import { errorMessage, issueInvoice } from '#lib/api.ts';
	import { buildInvoice, type LineForm } from '#lib/invoice-form.ts';
	import { centsToText, formatMoney, toCents, totals } from '#lib/money.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const emptyLine = (): LineForm => ({ code: '', description: '', quantity: '1', price: '', discount: '', vat: '15', product: '' });

	let lines = $state<LineForm[]>([emptyLine()]);
	let finalConsumer = $state(true);
	let buyer = $state({ idType: '05', idNumber: '', name: '', address: '', email: '' });
	let paymentMethod = $state('01');
	let busy = $state(false);
	let error = $state<string | null>(null);

	const productLabel = (p: { main_code: string; name: string }) => `${p.main_code} · ${p.name}`;

	/** Picking a catalog product fills the line. */
	function pickProduct(line: LineForm) {
		const p = data.products.find((x) => productLabel(x) === line.product);
		if (!p) return;
		line.code = p.main_code;
		line.description = p.name;
		line.price = centsToText(p.price_cents);
		line.vat = p.vat;
	}

	/** Typing a known ID number fills the buyer. */
	function pickCustomer() {
		const c = data.customers.find((x) => x.id_number === buyer.idNumber.trim());
		if (!c) return;
		buyer.idType = c.id_type;
		buyer.name = c.business_name;
		buyer.address = c.address ?? '';
		if (c.email) buyer.email = c.email;
	}

	let summary = $derived(
		totals(
			lines.map((l) => ({
				quantity: Number.isInteger(Number(l.quantity)) ? Number(l.quantity) : 0,
				price: toCents(l.price) ?? 0,
				discount: toCents(l.discount || '0') ?? 0,
				vat: l.vat
			}))
		)
	);

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		error = null;
		const built = buildInvoice({ finalConsumer, buyer, lines, paymentMethod });
		if ('error' in built) {
			error = built.error;
			return;
		}
		busy = true;
		try {
			const record = await issueInvoice(built.invoice);
			await goto(`/invoices/${record.access_key}`);
		} catch (e) {
			error = errorMessage(e);
		}
		busy = false;
	}
</script>

<a class="back" href="/">← Documents</a>
<div class="page-header"><h1>New invoice</h1></div>

{#if error}<div class="notice error">{error}</div>{/if}

<datalist id="product-options">
	{#each data.products as p (p.id)}<option value={productLabel(p)}></option>{/each}
</datalist>
<datalist id="customer-options">
	{#each data.customers as c (c.id)}<option value={c.id_number}>{c.business_name}</option>{/each}
</datalist>

<form onsubmit={submit}>
	<div class="form-layout">
		<div>
			<div class="card">
				<h2>Buyer</h2>
				<p>
					<label class="checkbox-label">
						<input type="checkbox" bind:checked={finalConsumer} />
						Final consumer
					</label>
				</p>
				{#if !finalConsumer}
					<div class="row">
						<div>
							<label for="id_number">ID number</label>
							<input id="id_number" list="customer-options" autocomplete="off" bind:value={buyer.idNumber} oninput={pickCustomer} />
						</div>
						<div>
							<label for="id_type">Type</label>
							<select id="id_type" bind:value={buyer.idType}>
								<option value="05">Cédula</option>
								<option value="04">RUC</option>
								<option value="06">Passport</option>
								<option value="08">Foreign ID</option>
							</select>
						</div>
						<div style="grid-column: span 2">
							<label for="name">Name / business name</label>
							<input id="name" bind:value={buyer.name} />
						</div>
					</div>
					<div class="row">
						<div>
							<label for="address">Address (optional)</label>
							<input id="address" bind:value={buyer.address} />
						</div>
					</div>
					{#if data.customers.length}
						<p class="hint">Type a saved ID number to fill in the details.</p>
					{/if}
				{/if}
				<div class="row" style="margin-top:.9rem">
					<div>
						<label for="email">Email (optional)</label>
						<input id="email" type="email" bind:value={buyer.email} />
					</div>
				</div>
			</div>

			<div class="card">
				<h2>Lines</h2>
				{#each lines as l, i (i)}
					<div class="line">
						{#if data.products.length}
							<div class="row">
								<div style="grid-column: 1 / -1">
									<label for="pr{i}">Catalog product (optional)</label>
									<input id="pr{i}" list="product-options" placeholder="Search by code or name…" autocomplete="off" bind:value={l.product} oninput={() => pickProduct(l)} />
								</div>
							</div>
						{/if}
						<div class="row">
							<div><label for="c{i}">Code</label><input id="c{i}" bind:value={l.code} placeholder="S/C" /></div>
							<div class="wide">
								<label for="d{i}">Description</label><input id="d{i}" bind:value={l.description} />
							</div>
							<div>
								<label for="q{i}">Quantity</label>
								<input id="q{i}" bind:value={l.quantity} inputmode="numeric" />
							</div>
							<div>
								<label for="p{i}">Unit price</label>
								<input id="p{i}" bind:value={l.price} inputmode="decimal" placeholder="0.00" />
							</div>
							<div>
								<label for="x{i}">Discount</label>
								<input id="x{i}" bind:value={l.discount} inputmode="decimal" placeholder="0.00" />
							</div>
							<div>
								<label for="v{i}">VAT</label>
								<select id="v{i}" bind:value={l.vat}>
									<option value="15">15 %</option>
									<option value="5">5 %</option>
									<option value="0">0 %</option>
									<option value="not_subject">Not subject</option>
									<option value="exempt">Exempt</option>
								</select>
							</div>
							<div style="align-self:end">
								{#if lines.length > 1}
									<button type="button" class="secondary small danger" onclick={() => lines.splice(i, 1)}>Remove</button>
								{/if}
							</div>
						</div>
					</div>
				{/each}
				<button type="button" class="secondary" onclick={() => lines.push(emptyLine())}>+ Add line</button>
			</div>

			<div class="card">
				<h2>Payment</h2>
				<div class="row">
					<div>
						<label for="payment">Payment method</label>
						<select id="payment" bind:value={paymentMethod}>
							<option value="01">Without financial system</option>
							<option value="16">Debit card</option>
							<option value="19">Credit card</option>
							<option value="20">Other, financial system</option>
							<option value="17">Electronic money</option>
						</select>
					</div>
				</div>
			</div>
		</div>

		<aside class="card summary">
			<h2>Summary</h2>
			<dl>
				<dt>Subtotal</dt><dd class="num">{formatMoney(summary.subtotal)}</dd>
				<dt>VAT</dt><dd class="num">{formatMoney(summary.vat)}</dd>
				<dt>Total</dt><dd class="num total">{formatMoney(summary.total)}</dd>
			</dl>
			<button disabled={busy}>{busy ? 'Sending to the SRI…' : 'Issue invoice'}</button>
		</aside>
	</div>
</form>
