<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { customers, errorMessage, type Customer } from '#lib/api.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	let failure = $state<string | null>(null);

	const ID_TYPES: Record<string, string> = { '04': 'RUC', '05': 'Cédula', '06': 'Passport', '08': 'Foreign' };
	const link = (p: number) => `?${new URLSearchParams({ q: data.q, page: String(p) })}`;

	async function remove(c: Customer) {
		if (!confirm(`Delete "${c.business_name}"?`)) return;
		failure = null;
		try {
			await customers.remove(c.id);
			await invalidateAll();
		} catch (e) {
			failure = errorMessage(e);
		}
	}
</script>

<div class="page-header">
	<div>
		<h1>Customers</h1>
		<p>Frequent buyers, to autofill the invoice.</p>
	</div>
	<a class="btn" href="/customers/new">New customer</a>
</div>

{#if data.error}<div class="notice error">{data.error}</div>{/if}
{#if failure}<div class="notice error">{failure}</div>{/if}

<form method="GET" class="card">
	<div class="row">
		<div><label for="q">Search by ID number or name</label><input id="q" name="q" value={data.q} autocomplete="off" /></div>
	</div>
	<div class="actions"><button>Search</button><a class="btn secondary" href="/customers">Clear</a></div>
</form>

<div class="card flush table-wrap">
	{#if data.items.length === 0 && !data.error}
		<div class="empty">No customers yet. <a href="/customers/new">Add the first one</a>.</div>
	{:else}
		<table>
			<thead><tr><th>Type</th><th>ID number</th><th>Name</th><th>Email</th><th>Phone</th><th></th></tr></thead>
			<tbody>
				{#each data.items as c (c.id)}
					<tr>
						<td>{ID_TYPES[c.id_type] ?? c.id_type}</td>
						<td class="mono">{c.id_number}</td>
						<td>{c.business_name}</td>
						<td>{c.email ?? ''}</td>
						<td>{c.phone ?? ''}</td>
						<td class="right">
							<span class="actions" style="justify-content:flex-end">
								<a class="btn secondary small" href="/customers/{c.id}">Edit</a>
								<button class="secondary small danger" onclick={() => remove(c)}>Delete</button>
							</span>
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
		<div class="table-footer">
			<span>{data.total} customers · page {data.page} of {data.pages}</span>
			<span class="actions">
				{#if data.page > 1}<a class="btn secondary small" href={link(data.page - 1)}>Previous</a>{/if}
				{#if data.page < data.pages}<a class="btn secondary small" href={link(data.page + 1)}>Next</a>{/if}
			</span>
		</div>
	{/if}
</div>
