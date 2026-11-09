# 📅 Kauwlendar

> **Dashboard de calendario visual y eficiente para Kindle con Jailbreak**, desarrollado en Rust para pantallas de tinta electrónica (E-Ink).

Kauwlendar convierte tu Kindle en un panel de agenda diario elegante, nítido y de bajo consumo. Diseñado para sincronizar eventos en vivo desde **Google Calendar, Outlook o Apple Calendar** (mediante enlaces privados iCal/ICS), mostrando un diseño gráfico optimizado para e-ink y permitiendo salir limpiamente al sistema de Kindle en cualquier momento mediante un toque en pantalla.

---

## ✨ Características

* **Sincronización en vivo al abrir:** Consulta tus eventos actualizados al instante sin depender de archivos de caché desfasados.
* **Soporte de múltiples calendarios:** Combina calendarios personales, de trabajo o familiares en una sola vista cronológica.
* **Soporte de eventos recurrentes (RRULE):** Reconoce eventos que se repiten diaria, semanal (ej. lunes y miércoles), mensual o anualmente (cumpleaños, aniversarios), así como cancelaciones (`EXDATE`).
* **Formato de 12h y 24h configurable:** Elige entre formato militar (`14:30`) o formato AM/PM (`02:30 PM`) desde tu configuración.
* **Diseño gráfico nítido para E-Ink:**
  * Generación gráfica a resolución nativa ($1072 \times 1448$ px para Paperwhite / escalable a todos los modelos).
  * Tipografía embebida de alta legibilidad (*Liberation Sans*), con soporte completo para tildes y caracteres en español.
  * Tarjetas de eventos estructuradas: sección para eventos de "Todo el día" y línea de tiempo para eventos con hora programada.
  * Indicador de porcentaje de batería del Kindle y hora de última sincronización.
* **Botón interactivo de salida `[ SALIR ]`:**
  * Detecta toques en la esquina superior derecha (`/dev/input/event*`) para salir ordenadamente.
  * Reactiva automáticamente la interfaz original del Kindle (`framework` y `powerd`), evitando pantallas congeladas.
* **Gestión de errores con salida segura:**
  * Si no hay conexión Wi-Fi, la URL falla o la configuración es incorrecta, muestra una pantalla de error amigable con botón de salida, en lugar de bloquear el Kindle.
* **Modo de desarrollo y pruebas en PC:**
  * Puedes probar y verificar la aplicación en tu computadora con `cargo run`.
  * Genera una imagen `preview.png` en tiempo real que muestra **exactamente** cómo se verá en la pantalla del Kindle.
* **Eficiencia de batería:**
  * Apaga el Wi-Fi tras sincronizar.
  * Suspende el dispositivo a memoria RAM mediante `rtcwake` durante los intervalos de actualización y entra en modo de reposo prolongado durante la noche (21:00 a 06:00).

---

## 📸 Vista Previa del Diseño

Al ejecutarse, Kauwlendar genera un panel dividido en tres secciones principales:

```
┌────────────────────────────────────────────────────────────────────────┐
│  Sunday, November 08, 2026                 10:30 AM  [85%]   [ EXIT ]  │
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│  TODO EL DÍA                                                           │
│  ────────────────────────────────────────────────────────────────────  │
│  [ Personal ]  All-Day Event                                           │
│                                                                        │
│  AGENDA DE HOY                                                         │
│  ────────────────────────────────────────────────────────────────────  │
│  [ 10:30 AM ]  [ Work     ]  Event 1                                   │
│  [ 02:15 PM ]  [ Personal ]  Event 2                                   │
│  [ 04:30 PM ]  [ Projects ]  Event 3                                   │
│                                                                        │
├────────────────────────────────────────────────────────────────────────┤
│  ↻ Sincronizado a las 10:30 AM  •  Próxima actualización en 60 min      │
│  Toca [SALIR] o presiona [q] para volver al Kindle                     │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 🛠️ Requisitos

* **Kindle con Jailbreak** (compatible con Kindle Paperwhite, Oasis, Basic y Voyage con firmware 5.x).
* **KUAL (Kindle Unified Application Launcher)** o terminal SSH para iniciar la aplicación.
* **FBInk** instalado (incluido por defecto si tienes KOReader en `/mnt/us/koreader/fbink` o instalable como extensión independiente).

---

## 🚀 Instalación en el Kindle

1. Conecta tu Kindle por USB a tu computadora.
2. Crea una carpeta llamada `kauwlendar` en la memoria del Kindle:
   ```text
   /mnt/us/kauwlendar/
   ```
3. Copia el binario compilado `kauwlendar` dentro de `/mnt/us/kauwlendar/`.
4. Copia tu archivo de configuración `calendars.toml` dentro de `/mnt/us/kauwlendar/`.
5. Copia el script lanzador `Kauwlendar.sh` en tu carpeta de documentos o scripts de KUAL:
   ```text
   /mnt/us/documents/Kauwlendar.sh
   ```
6. Dale permisos de ejecución en el Kindle (si accedes por SSH):
   ```bash
   chmod +x /mnt/us/kauwlendar/kauwlendar
   chmod +x /mnt/us/documents/Kauwlendar.sh
   ```

---

## ⚙️ Configuración (`calendars.toml`)

Crea o edita `/mnt/us/kauwlendar/calendars.toml` (o en la raíz del proyecto para pruebas locales):

```toml
# Intervalo de actualización en minutos durante el día (mínimo 5)
refresh_minutes = 60

# Diferencia horaria respecto a UTC (ej. 0 para UTC, -4 para UTC-4, -5 para UTC-5)
utc_offset_hours = 0

# Formato de visualización de hora:
# "12h" -> Formato AM/PM (ej. 10:30 AM, 02:15 PM)
# "24h" -> Formato 24 horas (ej. 10:30, 14:15)
time_format = "12h"

# Puedes agregar uno o más calendarios
[[calendars]]
name = "Personal"
url = "https://calendar.google.com/calendar/ical/YOUR_EMAIL/private-xxxx/basic.ics"

[[calendars]]
name = "Work"
url = "https://outlook.office365.com/owa/calendar/xxxx/reachcalendar.ics"
```

### 🔗 ¿Cómo obtener tu enlace iCal privado en Google Calendar?
1. Abre [Google Calendar](https://calendar.google.com/) en tu navegador.
2. En el panel izquierdo, busca el calendario que deseas y haz clic en los tres puntos `⋮` -> **Configurar y compartir**.
3. Baja hasta la sección **Integrar el calendario**.
4. Copia el enlace que dice **"Dirección secreta en formato iCal"**.
   > ⚠️ **Importante:** No uses la dirección pública a menos que tu calendario sea público. La dirección secreta permite a Kauwlendar leer tus eventos sin pedir inicio de sesión.

---

## 💻 Desarrollo y Pruebas en tu PC

Puedes probar todo el funcionamiento en tu máquina Linux/Mac/Windows sin tener conectado el Kindle:

1. Clona el repositorio:
   ```bash
   git clone git@github.com:arisness/kauwlendar.git
   cd kauwlendar
   ```
2. Crea tu archivo local `calendars.toml` (puedes copiar `calendars.toml.example`):
   ```bash
   cp calendars.toml.example calendars.toml
   ```
3. Ejecuta la aplicación en modo de prueba única:
   ```bash
   cargo run -- --oneshot
   ```
4. **Ver el resultado visual:**
   * La terminal imprimirá la agenda del día.
   * Se generará automáticamente el archivo `preview.png` en la raíz del proyecto.
   * Abre `preview.png` con cualquier visor de imágenes para comprobar la resolución y el diseño exacto de tinta electrónica.
5. Ejecutar los tests unitarios:
   ```bash
   cargo test
   ```

---

## 🔨 Compilación Cruzada para Kindle (ARM)

Para compilar el binario para la arquitectura del procesador del Kindle (`armv7-unknown-linux-musleabi`):

### Usando `cross` (Recomendado):
```bash
cargo install cross
cross build --target armv7-unknown-linux-musleabi --release
```
El binario resultante se encontrará en:
```text
target/armv7-unknown-linux-musleabi/release/kauwlendar
```

---

## 📄 Licencia

Este proyecto está bajo la licencia MIT. Siéntete libre de modificarlo y adaptarlo a tu dispositivo.

