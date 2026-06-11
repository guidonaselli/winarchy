-- managed by winarchy — generado desde templates/nvim-theme.lua.tpl. NO editar a mano.
-- Theme: {{name}}
-- Uso: require("winarchy-theme").apply()  (agregar config/nvim al runtimepath o copiar a lua/)
local M = {}

M.palette = {
  background = "{{colors.background}}",
  foreground = "{{colors.foreground}}",
  accent     = "{{colors.accent}}",
  cursor     = "{{colors.cursor}}",
  color0  = "{{colors.color0}}",  color1  = "{{colors.color1}}",
  color2  = "{{colors.color2}}",  color3  = "{{colors.color3}}",
  color4  = "{{colors.color4}}",  color5  = "{{colors.color5}}",
  color6  = "{{colors.color6}}",  color7  = "{{colors.color7}}",
  color8  = "{{colors.color8}}",  color9  = "{{colors.color9}}",
  color10 = "{{colors.color10}}", color11 = "{{colors.color11}}",
  color12 = "{{colors.color12}}", color13 = "{{colors.color13}}",
  color14 = "{{colors.color14}}", color15 = "{{colors.color15}}",
}

function M.apply()
  local p = M.palette
  vim.opt.termguicolors = true
  for i = 0, 15 do
    vim.g["terminal_color_" .. i] = p["color" .. i]
  end
  local set = vim.api.nvim_set_hl
  set(0, "Normal",       { fg = p.foreground, bg = p.background })
  set(0, "NormalFloat",  { fg = p.foreground, bg = p.color0 })
  set(0, "Cursor",       { fg = p.background, bg = p.cursor })
  set(0, "CursorLine",   { bg = p.color0 })
  set(0, "Visual",       { bg = p.color0 })
  set(0, "LineNr",       { fg = p.color8 })
  set(0, "CursorLineNr", { fg = p.accent, bold = true })
  set(0, "Comment",      { fg = p.color8, italic = true })
  set(0, "String",       { fg = p.color2 })
  set(0, "Number",       { fg = p.color3 })
  set(0, "Keyword",      { fg = p.color5 })
  set(0, "Function",     { fg = p.color4 })
  set(0, "Identifier",   { fg = p.foreground })
  set(0, "Type",         { fg = p.color6 })
  set(0, "Constant",     { fg = p.color3 })
  set(0, "Statement",    { fg = p.color5 })
  set(0, "Error",        { fg = p.color1 })
  set(0, "WarningMsg",   { fg = p.color3 })
  set(0, "DiagnosticError", { fg = p.color1 })
  set(0, "DiagnosticWarn",  { fg = p.color3 })
  set(0, "DiagnosticInfo",  { fg = p.color4 })
  set(0, "DiagnosticHint",  { fg = p.color6 })
  set(0, "Pmenu",        { fg = p.foreground, bg = p.color0 })
  set(0, "PmenuSel",     { fg = p.background, bg = p.accent })
  set(0, "StatusLine",   { fg = p.foreground, bg = p.color0 })
  set(0, "VertSplit",    { fg = p.color8 })
  set(0, "Search",       { fg = p.background, bg = p.color3 })
  set(0, "IncSearch",    { fg = p.background, bg = p.accent })
  set(0, "MatchParen",   { fg = p.accent, bold = true, underline = true })
end

return M
