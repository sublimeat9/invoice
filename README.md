# invoice-rust

Facturación electrónica para el SRI de Ecuador.

- `backend/`: Rust (clave de acceso, XML, firma XAdES-BES, cliente SOAP del SRI, SQLite y
  API HTTP con axum). Ejemplos en `backend/examples/` (`issue`, `query`, `sign`).
- `frontend/`: SvelteKit (SPA). Se usa de dos formas con el mismo código:
  - **Web**: un servidor Node sirve la interfaz y reenvía `/api/*` al backend añadiendo el token.
  - **Escritorio (Tauri)**: `frontend/src-tauri` empaqueta la interfaz con el backend dentro del
    mismo proceso; sin puerto, sin token y con los datos en la carpeta de la aplicación.

## Arranque local (web)
```
# 1) backend (desde backend/)
API_TOKEN=<≥16 caracteres> ENCRYPTION_KEY=<≥16 caracteres> cargo run

# 2) frontend (desde frontend/); mismo API_TOKEN
cp .env.example .env && pnpm install && pnpm dev
```

## Escritorio (Tauri)
```
cd frontend
pnpm install
pnpm tauri dev        # ventana con recarga en caliente
pnpm tauri build      # instalador para el sistema actual
```
Requiere Rust y, en Linux, `webkit2gtk-4.1` (ver <https://tauri.app/start/prerequisites/>).
La clave que cifra el certificado se guarda en el llavero del sistema (o, si no hay, en un
archivo con permisos 0600 junto a la base). Los instaladores para Windows, macOS y Linux los
genera `.github/workflows/desktop.yml`.

El emisor, el ambiente y el certificado `.p12` se configuran desde la página **Settings**
(el certificado se guarda cifrado; si se pierde la clave hay que volver a cargarlo). Productos y
clientes se administran desde sus páginas.

## API
Rutas en inglés (`/invoices`, `/invoices/report.csv`, `/invoices/retry`, `/config`,
`/config/issuer`, `/config/certificate`, `/products`, `/customers`), JSON con claves en inglés y
montos en centavos. Los estados del SRI (`AUTORIZADO`, `PENDIENTE`…) y el XML se mantienen como los
define la ficha técnica.

Pruebas: `cd backend && cargo test` · `cd frontend && pnpm check`
