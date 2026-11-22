<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { errorMessage, products, type VatCode } from '#lib/api.ts';
	import { centsToText, toCents } from '#lib/money.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const isNew = $derived(page.params.id === 'new');

	// The form starts from the loaded product; the page is remounted when navigating.
	/* svelte-ignore state_referenced_locally */
	const p = data.product;
	let mainCode = $state(p?.main_code ?? '');
	let auxCode = $state(p?.aux_code ?? '');
	let name = $state(p?.name ?? '');
	let price = $state(p ? centsToText(p.price_cents) : '');
	let vat = $state<VatCode>(p?.vat ?? '15');
	let busy = $state(false);
	/* svelte-ignore state_referenced_locally */
	let error = $state<string | null>(data.error);

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		error = null;
		const cents = toCents(price);
		if (cents === null || cents <= 0) {
			error = 'Invalid price (use up to 2 decimals)';
			return;
		}
		const body = { main_code: mainCode, aux_code: auxCode || null, name, price_cents: cents, vat };
		busy = true;
		try {
			if (isNew) await products.create(body);
			else await products.update(Number(page.params.id), body);
			await goto('/products');
		} catch (e) {
			error = errorMessage(e);
		}
		busy = false;
	}
</script>

<a class="back" href="/products">← Products</a>
<div class="page-header"><h1>{isNew ? 'New product' : 'Edit product'}</h1></div>

{#if error}<div class="notice error">{error}</div>{/if}

<form class="card" onsubmit={submit}>
	<div class="row">
		<div><label for="mc">Main code</label><input id="mc" maxlength="25" bind:value={mainCode} required /></div>
		<div><label for="ac">Aux. code (optional)</label><input id="ac" maxlength="25" bind:value={auxCode} /></div>
	</div>
	<div class="row">
		<div style="grid-column: span 2"><label for="n">Name / description</label><input id="n" bind:value={name} required /></div>
	</div>
	<div class="row">
		<div><label for="pr">Unit price</label><input id="pr" inputmode="decimal" placeholder="0.00" bind:value={price} required /></div>
		<div>
			<label for="vat">VAT</label>
			<select id="vat" bind:value={vat}>
				<option value="15">15 %</option><option value="5">5 %</option><option value="0">0 %</option>
				<option value="not_subject">Not subject to VAT</option><option value="exempt">Exempt from VAT</option>
			</select>
		</div>
	</div>
	<div class="actions"><button disabled={busy}>{busy ? 'Saving…' : 'Save'}</button><a class="btn secondary" href="/products">Cancel</a></div>
</form>
