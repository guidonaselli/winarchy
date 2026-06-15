# Releasing Winarchy

Winarchy actualiza a sus usuarios vía `winarchy update --self`, que hace
`git pull --ff-only origin release` + migración idempotente. Para que eso funcione, cada
release tiene que dejar la rama `release` apuntando al tag estable y la `ModuleVersion` en
sync con el tag.

## Fuente de la versión

- **`ModuleVersion`** en `module/Winarchy/Winarchy.psd1` es la verdad de "qué versión está
  instalada" (la lee `Get-WinarchyVersion` y el chequeo de updates).
- El **tag git `vX.Y.Z`** y la **GitHub Release** definen "qué es la última versión".
- Los tres deben coincidir en cada release: `ModuleVersion = X.Y.Z`, tag `vX.Y.Z`,
  Release `vX.Y.Z`.

## Pasos para publicar una versión

1. **Bump** de `ModuleVersion` en `Winarchy.psd1` (semver: MAJOR.MINOR.PATCH).
2. Commit en `main` con el changelog del cambio.
3. **Tag**: `git tag -a vX.Y.Z -m "winarchy vX.Y.Z"` y `git push origin vX.Y.Z`.
4. **Fast-forward de `release`** al tag (es lo que traccionan los usuarios):
   ```bash
   git checkout release
   git merge --ff-only vX.Y.Z
   git push origin release
   git checkout main
   ```
   Si `release` no se puede fast-forwardear (divergió), es un error de proceso: `release`
   solo debe avanzar linealmente sobre tags estables.
5. **GitHub Release**: crear la release `vX.Y.Z` con changelog (`gh release create vX.Y.Z
   --notes "..."`). La API `releases/latest` es la que consulta el chequeo no intrusivo.

## Notas

- El install oficial clona y trackea la rama `release` (no `main`), así los usuarios
  reciben solo versiones estampadas.
- El self-update corre `install.ps1 -SkipPackages` (sin `-Activate`) como migración: regenera
  configs desde templates y reaplica toggles sin cambiar el modo de operación vigente.
- Rollback de una release mala: el usuario hace `git checkout <tag-anterior>` + `install.ps1`.
