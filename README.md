# Gestor Git

Aplicación de escritorio para Windows que guarda las rutas de tus repositorios locales y ejecuta comandos git en todos ellos, en paralelo o en secuencia repo por repo.

Hecha con [Tauri 2](https://tauri.app/) (backend en Rust) y React + TypeScript.

## Estado

- [x] Paso 1: gestión de repositorios (añadir, buscar en una carpeta, agrupar, quitar) con configuración persistente.
- [x] Paso 2: motor de ejecución en paralelo y en secuencia, con salida en vivo por repo y cancelación.
- [x] Paso 3: secuencias de comandos guardadas y atajos (status, fetch, pull).
- [x] Paso 4: instalador de Windows (NSIS y MSI) generado por GitHub Actions.

## Instalar

Descarga el instalador desde [Releases](https://github.com/danielhdzf/Gestor-Git/releases):

- **`Gestor Git_x.y.z_x64-setup.exe`** (recomendado): se instala solo para tu usuario, sin pedir permisos de administrador.
- **`Gestor Git_x.y.z_x64_es-ES.msi`**: instalación clásica de Windows.

Necesitas `git` instalado y en el `PATH`. Windows puede avisar de que el instalador no está firmado: pulsa "Más información" y luego "Ejecutar de todas formas".

### Publicar una versión

1. Sube la versión en `package.json`, `src-tauri/Cargo.toml` y `src-tauri/tauri.conf.json`.
2. Crea y sube una etiqueta: `git tag v0.2.0 && git push origin v0.2.0`.
3. El workflow **Release** compila en Windows y deja un borrador en Releases con los instaladores. Revísalo y publícalo.

Cada PR también genera los instaladores. Están en la pestaña **Actions**, en la ejecución de **CI**, como artefacto `instaladores-windows`.

## Desarrollo

### Requisitos (Windows)

- [Node.js](https://nodejs.org/) 20 o superior.
- [Rust](https://www.rust-lang.org/tools/install) con el toolchain MSVC y las "Build Tools for Visual Studio" (C++).
- WebView2 (ya viene con Windows 10/11).
- `git` en el `PATH`.

Más detalles en los [prerrequisitos de Tauri](https://tauri.app/start/prerequisites/).

### Arrancar en modo desarrollo

```powershell
npm install
npm run tauri dev
```

### Tests del backend

```powershell
cd src-tauri
cargo test
```

La lista de repositorios y las secuencias se guardan en `%APPDATA%\com.danielhdzf.gestorgit\config.json`.
