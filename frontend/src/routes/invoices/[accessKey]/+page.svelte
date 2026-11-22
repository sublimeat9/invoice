<script lang="ts">
	import { errorMessage } from '#lib/api.ts';
	import { saveFile } from '#lib/files.ts';
	import { formatMoney } from '#lib/money.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	let invoice = $derived(data.invoice);
	let failure = $state<string | null>(null);

	async function downloadXml() {
		if (!invoice?.signed_xml) return;
		failure = null;
		try {
			await saveFile(`${invoice.access_key}.xml`, invoice.signed_xml, 'application/xml');
		} catch (e) {
			failure = errorMessage(e);
		}
	}
</script>

<a class="back" href="/">← Documents</a>

{#if data.error || !invoice}
	<div class="notice error">{data.error ?? 'Document not found'}</div>
{:else}
	<div class="page-header">
		<div>
			<h1>Invoice {invoice.number}</h1>
			<p><span class="badge {invoice.status.replaceAll(' ', '_')}">{invoice.status}</span></p>
		</div>
		<button class="secondary" onclick={downloadXml}>Download signed XML</button>
	</div>

	{#if failure}<div class="notice error">{failure}</div>{/if}

	<div class="card">
		<dl class="details">
			<dt>Issue date</dt><dd>{invoice.issue_date ?? '—'}</dd>
			<dt>Buyer</dt><dd>{invoice.business_name ?? '—'}{invoice.id_number ? ` · ${invoice.id_number}` : ''}</dd>
			<dt>Total</dt><dd class="num">{invoice.total_amount === null ? '—' : formatMoney(invoice.total_amount)}</dd>
			<dt>Access key</dt><dd class="mono">{invoice.access_key}</dd>
			<dt>Authorization number</dt><dd class="mono">{invoice.authorization_number ?? '—'}</dd>
			<dt>Authorization date</dt><dd>{invoice.authorization_date ?? '—'}</dd>
		</dl>
	</div>

	{#if invoice.messages}
		<div class="card"><h2>SRI messages</h2><pre>{invoice.messages}</pre></div>
	{/if}
{/if}
