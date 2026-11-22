<script lang="ts">
	import favicon from '#lib/assets/favicon.svg';
	import type { LayoutProps } from './$types';
	import './estilos.css';

	let { children }: LayoutProps = $props();
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<title>Facturación electrónica</title>
</svelte:head>

<header>
	<a href="/" class="marca">Facturación SRI</a>
	<nav>
		<a href="/">Facturas</a>
		<a href="/facturas/nueva" class="boton">Nueva factura</a>
	</nav>
</header>
<main>{@render children()}</main>
