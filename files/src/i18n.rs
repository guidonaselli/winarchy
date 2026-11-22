use std::sync::atomic::{AtomicU8, Ordering};

const ES: &[(&str, &str)] = &[
    ("Up", "Subir"),
    ("Address", "Dirección"),
    ("Go", "Ir"),
    ("New folder name", "Nombre de la carpeta nueva"),
    ("Create folder", "Crear carpeta"),
    ("New name for one selected item", "Nuevo nombre para el elemento seleccionado"),
    ("Rename selected", "Renombrar"),
    ("Delete to Recycle Bin", "Enviar a la Papelera"),
    ("Copy", "Copiar"),
    ("Cut", "Cortar"),
    ("Paste here", "Pegar aquí"),
    ("Destination folder", "Carpeta de destino"),
    ("Destination folder path", "Ruta de la carpeta de destino"),
    ("Copy selected", "Copiar selección"),
    ("Move selected", "Mover selección"),
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
    ("Archive failed", "Falló el archivo comprimido"),
    ("Archive already exists", "El archivo comprimido ya existe"),
    ("Could not run", "No se pudo ejecutar"),
    ("Item name", "Nombre del elemento"),
    ("Name for a new folder or file, or the new name", "Nombre de la carpeta o archivo nuevo, o el nombre nuevo"),
    ("Copy or move the selection to this folder", "Copiar o mover la selección a esta carpeta"),
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
