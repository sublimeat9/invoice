<script lang="ts">
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	let f = $derived(data.factura);
</script>

<p><a href="/">← Volver</a></p>
<h1>Factura {String(f.secuencial).padStart(9, '0')}</h1>

<div class="panel">
	<p>Estado: <span class="estado {f.estado.replaceAll(' ', '_')}">{f.estado}</span></p>
	<p>Clave de acceso: <span class="mono">{f.clave_acceso}</span></p>
	<p>Número de autorización: <span class="mono">{f.numero_autorizacion ?? '—'}</span></p>
	<p>Fecha de autorización: {f.fecha_autorizacion ?? '—'}</p>
	<p><a class="boton" href="/facturas/{f.clave_acceso}/xml" download>Descargar XML firmado</a></p>
</div>

{#if f.mensajes}
	<div class="panel"><strong>Mensajes del SRI</strong><pre>{f.mensajes}</pre></div>
{/if}
