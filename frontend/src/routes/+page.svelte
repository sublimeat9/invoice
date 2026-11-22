<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { page } from '$app/state';
	import { errorMessage, invoiceReportCsv, retryPending } from '#lib/api.ts';
	import { saveFile } from '#lib/files.ts';
	import { formatMoney } from '#lib/money.ts';
	import { filtersFrom } from './+page.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	let busy = $state(false);
	let notice = $state<{ kind: 'error' | 'info'; text: string } | null>(null);

	const DOCUMENT_TYPES: Record<string, string> = { '01': 'Invoice' };
	let hasPending = $derived(data.invoices.some((i) => i.status === 'PENDIENTE'));
	const badgeClass = (status: string) => status.replaceAll(' ', '_');
	const filter = (k: string) => (data.filters as Record<string, string>)[k] ?? '';

	async function retry() {
		busy = true;
		notice = null;
		try {
			const r = await retryPending();
			notice = { kind: 'info', text: `Retried ${r.length} pending documents.` };
			await invalidateAll();
		} catch (e) {
			notice = { kind: 'error', text: errorMessage(e) };
		}
		busy = false;
	}

	async function downloadCsv() {
		notice = null;
		try {
			await saveFile('documents.csv', await invoiceReportCsv(filtersFrom(new URL(page.url.href))), 'text/csv;charset=utf-8');
		} catch (e) {
			notice = { kind: 'error', text: errorMessage(e) };
		}
	}

	/** Link that keeps the current filters. */
	function link(extra: Record<string, string | number>): string {
		const p = new URLSearchParams(data.filters as Record<string, string>);
		for (const [k, v] of Object.entries(extra)) p.set(k, String(v));
		return `/?${p}`;
	}
</script>

<div class="page-header">
	<div>
		<h1>Documents</h1>
		<p>Search and download issued invoices.</p>
	</div>
	<a class="btn" href="/invoices/new">New invoice</a>
</div>

<form method="GET" class="card">
	<div class="row">
		<div><label for="date_from">From</label><input id="date_from" name="date_from" type="date" value={filter('date_from')} /></div>
		<div><label for="date_to">To</label><input id="date_to" name="date_to" type="date" value={filter('date_to')} /></div>
		<div>
			<label for="id_number">ID number</label>
			<input id="id_number" name="id_number" value={filter('id_number')} autocomplete="off" />
		</div>
		<div>
			<label for="access_key">Access key / authorization</label>
			<input id="access_key" name="access_key" value={filter('access_key')} autocomplete="off" />
		</div>
		<div>
			<label for="status">Status</label>
			<select id="status" name="status" value={filter('status')}>
				<option value="">All</option>
				<option>AUTORIZADO</option>
				<option>PENDIENTE</option>
				<option>RECHAZADO</option>
				<option>DEVUELTA</option>
				<option value="NO AUTORIZADO">NO AUTORIZADO</option>
			</select>
		</div>
	</div>
	<div class="actions">
		<button>Search</button>
		<a href="/" class="btn secondary">Clear</a>
	</div>
</form>

{#if data.error}<div class="notice error">{data.error}</div>{/if}
{#if notice}<div class="notice {notice.kind}">{notice.text}</div>{/if}

{#if hasPending}
	<div class="notice">
		<span>Some invoices are pending: the SRI did not answer.</span>
		<button class="secondary small" disabled={busy} onclick={retry}>
			{busy ? 'Retrying…' : 'Retry now'}
		</button>
	</div>
{/if}

<div class="card flush table-wrap">
	{#if data.invoices.length === 0 && !data.error}
		<div class="empty">No results found.</div>
	{:else}
		<table>
			<thead>
				<tr>
					<th>Date</th><th>Type</th><th>Number</th><th>ID number</th>
					<th>Name</th><th>Authorization</th><th>Status</th>
					<th class="right">Total</th>
				</tr>
			</thead>
			<tbody>
				{#each data.invoices as r (r.access_key)}
					<tr>
						<td>{r.issue_date ?? '—'}</td>
						<td>{DOCUMENT_TYPES[r.document_type] ?? r.document_type}</td>
						<td>{r.number}</td>
						<td>{r.id_number ?? '—'}</td>
						<td class="truncate">{r.business_name ?? '—'}</td>
						<td class="mono truncate"><a href="/invoices/{r.access_key}" title={r.authorization_number ?? r.access_key}>{r.authorization_number ?? r.access_key}</a></td>
						<td><span class="badge {badgeClass(r.status)}">{r.status}</span></td>
						<td class="right num">{r.total_amount === null ? '—' : formatMoney(r.total_amount)}</td>
					</tr>
				{/each}
			</tbody>
		</table>
		<div class="table-footer">
			<span>{data.total} results · page {data.page} of {data.pages}</span>
			<span class="actions">
				{#if data.page > 1}<a class="btn secondary small" href={link({ page: data.page - 1 })}>Previous</a>{/if}
				{#if data.page < data.pages}<a class="btn secondary small" href={link({ page: data.page + 1 })}>Next</a>{/if}
				<button class="secondary small" onclick={downloadCsv}>Download CSV</button>
			</span>
		</div>
	{/if}
</div>
