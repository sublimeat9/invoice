<script lang="ts">
	import favicon from '#lib/assets/favicon.svg';
	import { page } from '$app/state';
	import type { LayoutProps } from './$types';
	import './estilos.css';

	let { children }: LayoutProps = $props();
	const actual = (ruta: string) => (page.url.pathname === ruta ? 'page' : undefined);
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<title>Facturación electrónica</title>
</svelte:head>

<div class="app">
	<aside class="lateral">
		<a href="/" class="marca"><span class="logo">F</span> Facturación</a>
		<nav aria-label="Principal">
			<a href="/" aria-current={actual('/')}>
				<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true">
					<path d="M3 2.5h7l3 3v8H3z M10 2.5v3h3 M5.5 8.5h5 M5.5 11h5" />
				</svg>
				Comprobantes
			</a>
			<a href="/facturas/nueva" aria-current={actual('/facturas/nueva')}>
				<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true">
					<path d="M8 3v10 M3 8h10" />
				</svg>
				Nueva factura
			</a>
		</nav>
	</aside>
	<main class="contenido">{@render children()}</main>
</div>
