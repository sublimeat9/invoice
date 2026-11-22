# Frontend (SvelteKit)

Interfaz para emitir y consultar facturas. Habla con la API del `backend/` **desde el
servidor de SvelteKit**: el token nunca llega al navegador y no hace falta CORS.

## Desarrollo
```
cp .env.example .env     # mismo API_TOKEN que el backend
pnpm install
pnpm dev
```

Variables (solo servidor): `API_TOKEN` (≥16 caracteres) y `BACKEND_URL`
(por defecto `http://127.0.0.1:3000`).

## Páginas
- `/`: lista de facturas; botón para reintentar las pendientes.
- `/facturas/nueva`: formulario (comprador, líneas, IVA, forma de pago, correo).
- `/facturas/{clave}`: detalle, mensajes del SRI y descarga del XML firmado.
