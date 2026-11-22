# Frontend (SvelteKit)

Aplicación de una sola página (`ssr = false`) para emitir y consultar facturas. Toda la comunicación
con el backend pasa por `src/lib/api.ts`, que elige el transporte:

| Entorno | Transporte |
|---------|------------|
| Web | `fetch('/api/*')` → `src/hooks.server.ts` lo reenvía al backend con el token de sesión del usuario |
| Tauri | `invoke('api')` → el router del backend dentro del mismo proceso |

## Desarrollo
```
cp .env.example .env
pnpm install
pnpm dev                 # web
```

Variable (solo servidor web): `BACKEND_URL`
(por defecto `http://127.0.0.1:3000`).

## Compilación
- `pnpm build`: servidor Node (`adapter-node`) con el proxy `/api`.
- `pnpm build:tauri`: bundle estático (`adapter-static`) para la app de escritorio.
El instalador de escritorio ya no vive aquí: está en `../desktop`, que usa este bundle.

## Páginas
- `/`: comprobantes con filtros, paginación, reintento de pendientes y reporte CSV.
- `/invoices/new`: formulario (comprador, líneas, IVA, forma de pago, correo).
- `/invoices/{accessKey}`: detalle, mensajes del SRI y descarga del XML firmado.
- `/products`, `/customers`: catálogos (CRUD).
- `/settings`: emisor, ambiente y certificado `.p12`.
