# invoice-rust

Facturación electrónica para el SRI de Ecuador.

- `backend/`: Rust (clave de acceso, XML, firma XAdES-BES, cliente SOAP del SRI, SQLite y
  API HTTP con axum). Ejemplos en `backend/examples/` (`issue`, `query`, `sign`).
- `android/`: cliente nativo (Kotlin + Jetpack Compose) que consume la misma API.
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

## Android
```
cd android
./gradlew assembleDebug                      # app/build/outputs/apk/debug/app-debug.apk
adb install -r app/build/outputs/apk/debug/app-debug.apk
```
En **Settings** de la app se indican la URL del servidor y el `API_TOKEN` (el token se guarda cifrado con
una clave del Android Keystore). En el emulador, el backend local es `http://10.0.2.2:3000`; fuera de la
red local use HTTPS (el HTTP en claro solo está permitido para `10.0.2.2`, `localhost` y `127.0.0.1`).
Muestra comprobantes (filtros, reintento, compartir XML), emite facturas y administra productos y
clientes. El emisor y el certificado `.p12` se configuran desde la web o el escritorio, para que el
certificado no pase por el teléfono.

## API
Rutas en inglés (`/invoices`, `/invoices/report.csv`, `/invoices/retry`, `/config`,
`/config/issuer`, `/config/certificate`, `/products`, `/customers`), JSON con claves en inglés y
montos en centavos. Los estados del SRI (`AUTORIZADO`, `PENDIENTE`…) y el XML se mantienen como los
define la ficha técnica.

Pruebas: `cd backend && cargo test` · `cd frontend && pnpm check`
