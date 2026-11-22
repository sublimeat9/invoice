# Frontend (SvelteKit)

Aplicación de una sola página (`ssr = false`) para emitir y consultar facturas. Toda la comunicación
con el backend pasa por `src/lib/api.ts`, que elige el transporte:

| Entorno | Transporte |
|---------|------------|
| Web | `fetch('/api/*')` → `src/hooks.server.ts` lo reenvía al backend con el token (nunca llega al navegador) |
| Tauri | `invoke('api')` → el router del backend dentro del mismo proceso |

## Desarrollo
```
cp .env.example .env     # mismo API_TOKEN que el backend
pnpm install
pnpm dev                 # web
pnpm tauri dev           # escritorio
```

Variables (solo servidor web): `API_TOKEN` (≥16 caracteres) y `BACKEND_URL`
(por defecto `http://127.0.0.1:3000`).

## Compilación
- `pnpm build`: servidor Node (`adapter-node`) con el proxy `/api`.
- `pnpm build:tauri`: bundle estático (`adapter-static`) para la app de escritorio.
- `pnpm tauri build`: instalador (ejecuta `build:tauri` antes).

## Páginas
- `/`: comprobantes con filtros, paginación, reintento de pendientes y reporte CSV.
- `/invoices/new`: formulario (comprador, líneas, IVA, forma de pago, correo).
- `/invoices/{accessKey}`: detalle, mensajes del SRI y descarga del XML firmado.
- `/products`, `/customers`: catálogos (CRUD).
- `/settings`: emisor, ambiente y certificado `.p12`.
