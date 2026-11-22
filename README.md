# invoice-rust

Facturación electrónica para el SRI de Ecuador.

- `backend/`: Rust (clave de acceso, XML, firma XAdES-BES, cliente SOAP del SRI, SQLite y
  API HTTP con axum). Ejemplos en `backend/examples/`.
- `frontend/`: SvelteKit; consume la API del backend desde el servidor.

## Arranque local
```
# 1) backend (desde backend/): ver variables en src/main.rs
cargo run

# 2) frontend (desde frontend/)
pnpm dev
```
Pruebas: `cd backend && cargo test` · `cd frontend && pnpm check`
