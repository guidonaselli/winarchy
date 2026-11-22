use std::sync::atomic::{AtomicU8, Ordering};

const ES: &[(&str, &str)] = &[
    ("Up", "Subir"),
    ("Address", "Dirección"),
    ("New folder name", "Nombre de la carpeta nueva"),
    ("Create folder", "Crear carpeta"),
    ("New name for one selected item", "Nuevo nombre para el elemento seleccionado"),
    ("Rename selected", "Renombrar"),
    ("Delete to Recycle Bin", "Enviar a la Papelera"),
    ("Copy", "Copiar"),
    ("Cut", "Cortar"),
    ("Paste here", "Pegar aquí"),
    ("Destination folder", "Carpeta de destino"),
    ("Filter by name", "Filtrar por nombre"),
    ("Name", "Nombre"),
    ("Size", "Tamaño"),
    ("Modified", "Modificado"),
    ("Show hidden items", "Mostrar elementos ocultos"),
    ("Hidden: shown", "Ocultos: visibles"),
    ("Hidden: off", "Ocultos: no"),
    ("Search subfolders", "Buscar en subcarpetas"),
    ("Subfolders: on", "Subcarpetas: sí"),
    ("Subfolders: off", "Subcarpetas: no"),
    ("New window", "Nueva ventana"),
    ("Copy here", "Copiar aquí"),
    ("Select", "Seleccionar"),
    ("Home", "Inicio"),
    ("Desktop", "Escritorio"),
    ("Documents", "Documentos"),
    ("Downloads", "Descargas"),
    ("Pictures", "Imágenes"),
    ("Music", "Música"),
    ("Videos", "Vídeos"),
    ("Folder", "Carpeta"),
    ("File", "Archivo"),
    ("Language: auto", "Idioma: automático"),
    ("Language: Spanish", "Idioma: español"),
    ("Language: English", "Idioma: inglés"),
    ("Change language", "Cambiar idioma"),
    ("New tab", "Nueva pestaña"),
    ("Close tab", "Cerrar pestaña"),
    ("Tab", "Pestaña"),
    ("Back", "Atrás"),
    ("Forward", "Adelante"),
    ("Create file", "Crear archivo"),
    ("New folder or file name", "Nombre de carpeta o archivo nuevo"),
    ("Enter a single file name", "Escribí un solo nombre de archivo"),
    ("Could not create file", "No se pudo crear el archivo"),
    ("Copy path", "Copiar ruta"),
    ("Path copied", "Ruta copiada"),
    ("Terminal here", "Terminal aquí"),
    ("Could not open terminal", "No se pudo abrir la terminal"),
    ("Duplicate tab", "Duplicar pestaña"),
    ("Reopen closed tab", "Reabrir pestaña cerrada"),
    ("Close other tabs", "Cerrar las demás pestañas"),
    ("Delete permanently", "Eliminar definitivamente"),
    ("Deleting permanently…", "Eliminando definitivamente…"),
    ("Copy path with quotes", "Copiar ruta con comillas"),
    ("Show in folder", "Mostrar en su carpeta"),
    ("Shortcut", "Acceso directo"),
    ("Create shortcut", "Crear acceso directo"),
    ("Could not create shortcut", "No se pudo crear el acceso directo"),
    ("Reset column widths", "Restablecer ancho de columnas"),
    ("Sort by name", "Ordenar por nombre"),
    ("Sort by name ▲", "Ordenar por nombre ▲"),
    ("Sort by name ▼", "Ordenar por nombre ▼"),
    ("Sort by size", "Ordenar por tamaño"),
    ("Sort by size ▲", "Ordenar por tamaño ▲"),
    ("Sort by size ▼", "Ordenar por tamaño ▼"),
    ("Sort by date", "Ordenar por fecha"),
    ("Sort by date ▲", "Ordenar por fecha ▲"),
    ("Sort by date ▼", "Ordenar por fecha ▼"),
    ("Share", "Compartir"),
    ("Subfolders included: click to search only this folder", "Subcarpetas incluidas: clic para buscar solo en esta carpeta"),
    ("Click to include subfolders in the list and the filter", "Clic para incluir subcarpetas en la lista y el filtro"),
    ("Searching…", "Buscando…"),
    ("limited to the first", "limitado a los primeros"),
    ("Show more options", "Mostrar más opciones"),
    ("Favorites", "Favoritos"),
    ("Devices", "Dispositivos"),
    ("Eject", "Expulsar"),
    ("Could not eject", "No se pudo expulsar"),
    ("Drag failed", "No se pudo arrastrar"),
    ("Add to favorites", "Agregar a favoritos"),
    ("Remove from favorites", "Quitar de favoritos"),
    ("This folder is empty", "Esta carpeta está vacía"),
    ("No items match the filter", "Ningún elemento coincide con el filtro"),
    ("Subfolders", "Subcarpetas"),
    ("Settings file", "Archivo de configuración"),
    ("New", "Nuevo"),
    ("Sort", "Ordenar"),
    ("More", "Más"),
    ("Details", "Detalles"),
    ("Preview pane", "Panel de vista previa"),
    ("Refresh", "Actualizar"),
    ("Sort by", "Ordenar por"),
    ("New file name", "Nombre del archivo nuevo"),
    ("New name", "Nombre nuevo"),
    ("Tags, separated by commas", "Etiquetas, separadas por comas"),
    ("Repository URL", "URL del repositorio"),
    ("Copy to this folder", "Copiar a esta carpeta"),
    ("Move to this folder", "Mover a esta carpeta"),
    ("OK", "Aceptar"),
    ("Cancel", "Cancelar"),
    ("Edit address", "Editar dirección"),
    ("Add tags", "Agregar etiquetas"),
    ("Git clone", "Git: clonar"),
    ("Copy to…", "Copiar a…"),
    ("Move to…", "Mover a…"),
    ("Type at least one tag", "Escribí al menos una etiqueta"),
    ("Type the repository URL", "Escribí la URL del repositorio"),
    ("Preview pane: on", "Panel de vista previa: sí"),
    ("Preview pane: off", "Panel de vista previa: no"),
    ("Clear tags", "Quitar etiquetas"),
    ("Tags", "Etiquetas"),
    ("Select files to share", "Seleccioná archivos para compartir"),
    ("Could not share", "No se pudo compartir"),
    ("Select none", "Quitar selección"),
    ("Invert selection", "Invertir selección"),
    ("Extensions: shown", "Extensiones: visibles"),
    ("Extensions: hidden", "Extensiones: ocultas"),
    ("Show file extensions", "Mostrar extensiones de archivo"),
    ("Compress to zip", "Comprimir a zip"),
    ("Extract here", "Extraer aquí"),
    ("Edit in Notepad", "Editar en Bloc de notas"),
    ("Properties", "Propiedades"),
    ("Terminal as administrator", "Terminal como administrador"),
    ("Open Recycle Bin", "Abrir la Papelera (restaurar elementos)"),
    ("Empty Recycle Bin", "Vaciar la Papelera"),
    ("Layout: details", "Diseño: detalles"),
    ("Layout: list", "Diseño: lista"),
    ("Layout: tiles", "Diseño: mosaicos"),
    ("Layout: grid", "Diseño: cuadrícula"),
    ("Pin to sidebar", "Anclar a la barra lateral"),
    ("Unpin from sidebar", "Quitar de la barra lateral"),
    ("Open in editor", "Abrir en el editor (VS Code)"),
    ("Storage settings", "Configuración de almacenamiento"),
    ("Group: none", "Agrupar: no"),
    ("Group: type", "Agrupar: tipo"),
    ("Group: date", "Agrupar: fecha"),
    ("Group: size", "Agrupar: tamaño"),
    ("Folders", "Carpetas"),
    ("No extension", "Sin extensión"),
    ("Today", "Hoy"),
    ("Yesterday", "Ayer"),
    ("Last 7 days", "Últimos 7 días"),
    ("Last 30 days", "Últimos 30 días"),
    ("Last year", "Último año"),
    ("Older", "Anteriores"),
    ("Empty", "Vacíos"),
    ("Under 1 MB", "Menos de 1 MB"),
    ("1 to 100 MB", "De 1 a 100 MB"),
    ("Over 100 MB", "Más de 100 MB"),
    ("Paste as shortcut", "Pegar como acceso directo"),
    ("Git init", "Git: inicializar repositorio"),
    ("Git fetch", "Git: fetch"),
    ("Git pull", "Git: pull"),
    ("Git push", "Git: push"),
    ("Archive failed", "Falló el archivo comprimido"),
    ("Archive already exists", "El archivo comprimido ya existe"),
    ("Could not run", "No se pudo ejecutar"),
    ("item", "elemento"),
    ("items", "elementos"),
    ("selected|one", "seleccionado"),
    ("selected", "seleccionados"),
    ("of", "de"),
    ("cut", "cortados"),
    ("copied", "copiados"),
    ("Loading folder…", "Cargando carpeta…"),
    ("Copying dropped items…", "Copiando los elementos soltados…"),
    ("Redone", "Rehecho"),
    ("Undone", "Deshecho"),
    ("Undo canceled", "Deshacer cancelado"),
    ("Nothing to undo", "No hay nada que deshacer"),
    ("Nothing to redo", "No hay nada que rehacer"),
    ("Pasting…", "Pegando…"),
    ("Undoing…", "Deshaciendo…"),
    ("Redoing…", "Rehaciendo…"),
    ("Deleting to Recycle Bin…", "Enviando a la Papelera…"),
    ("Enter a destination folder path", "Ingrese la ruta de la carpeta de destino"),
    ("Moving selected items…", "Moviendo los elementos seleccionados…"),
    ("Copying selected items…", "Copiando los elementos seleccionados…"),
    ("Move completed", "Movimiento completado"),
    ("Copy completed", "Copia completada"),
    ("Operation canceled; some items may have transferred", "Operación cancelada; es posible que algunos elementos ya se hayan transferido"),
    ("Select one item to rename", "Seleccione un elemento para renombrar"),
    ("Enter a single item name", "Ingrese un único nombre de elemento"),
    ("Cannot rename this item", "No se puede renombrar este elemento"),
    ("Renaming…", "Renombrando…"),
    ("Enter a single folder name", "Ingrese un único nombre de carpeta"),
    ("Creating folder…", "Creando carpeta…"),
    ("Winarchy theme file is missing", "Falta el archivo de tema de Winarchy"),
    ("Clipboard failed", "Falló el portapapeles"),
    ("Undo failed", "Falló deshacer"),
    ("New window failed", "Falló abrir la ventana nueva"),
    ("Delete failed", "Falló el borrado"),
    ("File operation failed", "Falló la operación de archivos"),
    ("Rename failed", "Falló renombrar"),
    ("Theme watch failed", "Falló la vigilancia del tema"),
    ("Winarchy theme could not be loaded", "No se pudo cargar el tema de Winarchy"),
    ("Keyboard shortcuts unavailable", "Atajos de teclado no disponibles"),
    ("Opened in a new window", "Abierto en una ventana nueva"),
    ("Path", "Ruta"),
    ("Created", "Creado"),
    ("Attributes", "Atributos"),
    ("None", "Ninguno"),
    ("Read-only", "Solo lectura"),
    ("Hidden", "Oculto"),
    ("System", "Sistema"),
    ("Archive", "Archivar"),
];

static CHOICE: AtomicU8 = AtomicU8::new(0);

pub fn choices() -> [&'static str; 3] {
    ["auto", "es", "en"]
}

pub fn set_choice(name: &str) {
    let index = choices().iter().position(|choice| *choice == name).unwrap_or(0);
    CHOICE.store(index as u8, Ordering::Relaxed);
}

pub fn choice() -> &'static str {
    choices()[CHOICE.load(Ordering::Relaxed) as usize]
}

// The environment variable wins so scripts and tests stay deterministic.
fn spanish() -> bool {
    match std::env::var("WINARCHY_FILES_LANG") {
        Ok(lang) => lang.to_lowercase().starts_with("es"),
        Err(_) => match choice() {
            "es" => true,
            "en" => false,
            _ => system_language_is_spanish(),
        },
    }
}

fn system_language_is_spanish() -> bool {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buffer = [0u16; 85];
    let length = unsafe { GetUserDefaultLocaleName(&mut buffer) };
    length > 2 && String::from_utf16_lossy(&buffer[..2]).eq_ignore_ascii_case("es")
}

// Unknown text falls back to the English original, so a missing entry is never blank.
// A `|suffix` only tells apart keys that read the same in English ("selected|one").
pub fn tr_dynamic(english: &str) -> String {
    let plain = english.split('|').next().unwrap_or(english);
    if !spanish() {
        return plain.to_owned();
    }
    ES.iter().find(|(key, _)| *key == english).map_or(plain, |(_, text)| text).to_owned()
}

pub fn tr(english: &'static str) -> &'static str {
    let plain = english.split('|').next().unwrap_or(english);
    if !spanish() {
        return plain;
    }
    ES.iter()
        .find(|(key, _)| *key == english)
        .map_or(plain, |(_, text)| text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_has_unique_non_empty_keys() {
        let mut keys: Vec<_> = ES.iter().map(|(key, text)| {
            assert!(!key.is_empty() && !text.is_empty());
            *key
        }).collect();
        keys.sort_unstable();
        let total = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), total);
    }

    #[test]
    fn unknown_text_stays_english() {
        assert_eq!(tr("Not in the table"), "Not in the table");
    }
}
