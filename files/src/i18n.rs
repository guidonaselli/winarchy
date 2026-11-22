use std::sync::OnceLock;

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
    ("items", "elementos"),
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

fn spanish() -> bool {
    static SPANISH: OnceLock<bool> = OnceLock::new();
    *SPANISH.get_or_init(|| match std::env::var("WINARCHY_FILES_LANG") {
        Ok(lang) => lang.to_lowercase().starts_with("es"),
        Err(_) => system_language_is_spanish(),
    })
}

fn system_language_is_spanish() -> bool {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buffer = [0u16; 85];
    let length = unsafe { GetUserDefaultLocaleName(&mut buffer) };
    length > 2 && String::from_utf16_lossy(&buffer[..2]).eq_ignore_ascii_case("es")
}

// Unknown text falls back to the English original, so a missing entry is never blank.
pub fn tr(english: &'static str) -> &'static str {
    if !spanish() {
        return english;
    }
    ES.iter()
        .find(|(key, _)| *key == english)
        .map_or(english, |(_, text)| text)
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
