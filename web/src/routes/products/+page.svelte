<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { errorMessage, products, type Product } from '#lib/api.ts';
	import { formatMoney } from '#lib/money.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	let failure = $state<string | null>(null);

	const VAT: Record<string, string> = { '15': '15 %', '5': '5 %', '0': '0 %', not_subject: 'Not subject', exempt: 'Exempt' };
	const link = (p: number) => `?${new URLSearchParams({ q: data.q, page: String(p) })}`;

	async function remove(p: Product) {
		if (!confirm(`Delete "${p.name}"?`)) return;
		failure = null;
		try {
			await products.remove(p.id);
			await invalidateAll();
		} catch (e) {
			failure = errorMessage(e);
		}
	}
</script>

<div class="page-header">
	<div>
		<h1>Products and services</h1>
		<p>A catalog to invoice faster.</p>
	</div>
	<a class="btn" href="/products/new">New product</a>
</div>

{#if data.error}<div class="notice error">{data.error}</div>{/if}
{#if failure}<div class="notice error">{failure}</div>{/if}

<form method="GET" class="card">
	<div class="row">
		<div><label for="q">Search by code or name</label><input id="q" name="q" value={data.q} autocomplete="off" /></div>
	</div>
	<div class="actions"><button>Search</button><a class="btn secondary" href="/products">Clear</a></div>
</form>

<div class="card flush table-wrap">
	{#if data.items.length === 0 && !data.error}
		<div class="empty">No products yet. <a href="/products/new">Create the first one</a>.</div>
	{:else}
		<table>
			<thead><tr><th>Code</th><th>Aux.</th><th>Name</th><th>VAT</th><th class="right">Price</th><th></th></tr></thead>
			<tbody>
				{#each data.items as p (p.id)}
					<tr>
						<td class="mono">{p.main_code}</td>
						<td class="mono">{p.aux_code ?? ''}</td>
						<td>{p.name}</td>
						<td>{VAT[p.vat] ?? p.vat}</td>
						<td class="right num">{formatMoney(p.price_cents)}</td>
						<td class="right">
							<span class="actions" style="justify-content:flex-end">
								<a class="btn secondary small" href="/products/{p.id}">Edit</a>
								<button class="secondary small danger" onclick={() => remove(p)}>Delete</button>
							</span>
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
		<div class="table-footer">
			<span>{data.total} products · page {data.page} of {data.pages}</span>
			<span class="actions">
				{#if data.page > 1}<a class="btn secondary small" href={link(data.page - 1)}>Previous</a>{/if}
				{#if data.page < data.pages}<a class="btn secondary small" href={link(data.page + 1)}>Next</a>{/if}
			</span>
		</div>
	{/if}
</div>
