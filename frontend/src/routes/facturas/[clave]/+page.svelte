<script lang="ts">
	import { formatear } from '#lib/dinero.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	let f = $derived(data.factura);
</script>

<a class="volver" href="/">← Comprobantes</a>
<div class="cabecera">
	<div>
		<h1>Factura {f.numero}</h1>
		<p><span class="etiqueta {f.estado.replaceAll(' ', '_')}">{f.estado}</span></p>
	</div>
	<a class="boton sec" href="/facturas/{f.clave_acceso}/xml" download>Descargar XML firmado</a>
</div>

<div class="tarjeta">
	<dl class="datos">
		<dt>Fecha de emisión</dt><dd>{f.fecha_emision ?? '—'}</dd>
		<dt>Comprador</dt><dd>{f.razon_social ?? '—'}{f.identificacion ? ` · ${f.identificacion}` : ''}</dd>
		<dt>Valor total</dt><dd class="num">{f.valor_total === null ? '—' : formatear(f.valor_total)}</dd>
		<dt>Clave de acceso</dt><dd class="mono">{f.clave_acceso}</dd>
		<dt>Nro. de autorización</dt><dd class="mono">{f.numero_autorizacion ?? '—'}</dd>
		<dt>Fecha de autorización</dt><dd>{f.fecha_autorizacion ?? '—'}</dd>
	</dl>
</div>

{#if f.mensajes}
	<div class="tarjeta"><h2>Mensajes del SRI</h2><pre>{f.mensajes}</pre></div>
{/if}
