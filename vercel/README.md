# Backend en Vercel

La misma API del backend, empaquetada como función de Vercel (runtime de Rust, en beta) y con
los datos en Postgres (Supabase) en lugar de un archivo SQLite.

- `api/index.rs`: único punto de entrada; `vercel.json` envía todas las rutas allí.
- **Base de datos:** `DATABASE_URL` apunta a Supabase. Las tablas se crean solas al arrancar.
  Con el pooler en modo transacción (puerto 6543) funciona: no se usan sentencias preparadas.
- **Reintentos:** al no haber un proceso permanente, un cron de Vercel llama a
  `GET /internal/retry` con `Authorization: Bearer $CRON_SECRET`. Sin `CRON_SECRET`, esa ruta no
  existe. La frecuencia por defecto es diaria (el plan gratuito no admite más); en planes de pago
  puede ser cada pocos minutos.
- **Límite de intentos de login:** se guarda en la base, así vale entre instancias.
- **Variables** (en el proyecto de Vercel): `DATABASE_URL`, `ENCRYPTION_KEY` (≥ 16 caracteres;
  si se pierde, los certificados guardados no se pueden descifrar), `CRON_SECRET` y, opcionales,
  `ADMIN_USER` y `ADMIN_PASSWORD`.
- El proyecto de Vercel debe tener `vercel` como directorio raíz, con la opción de incluir
  archivos fuera de ese directorio (usa `../backend`).

La web puede ir en otro proyecto de Vercel; su variable `BACKEND_URL` apunta a esta función.
