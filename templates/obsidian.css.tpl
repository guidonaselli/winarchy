/* managed by winarchy — generado desde templates/obsidian.css.tpl. NO editar a mano. */
/* Theme: {{name}} */

.theme-dark, .theme-light {
  /* Core colors */
  --background-primary: {{colors.background}};
  --background-primary-alt: {{colors.background}};
  --background-secondary: {{colors.background}};
  --background-secondary-alt: {{colors.background}};
  --text-normal: {{colors.foreground}};

  /* Selection colors */
  --text-selection: {{colors.selection_background}};

  /* Border color */
  --background-modifier-border: {{colors.color8}};

  /* Semantic heading colors */
  --text-title-h1: {{colors.color1}};
  --text-title-h2: {{colors.color2}};
  --text-title-h3: {{colors.color3}};
  --text-title-h4: {{colors.color4}};
  --text-title-h5: {{colors.color5}};
  --text-title-h6: {{colors.color5}};

  /* Links and accents */
  --text-link: {{colors.color4}};
  --text-accent: {{colors.accent_ui}};
  --text-accent-hover: {{colors.accent_ui}};
  --interactive-accent: {{colors.accent_ui}};
  --interactive-accent-hover: {{colors.accent_ui}};
  --text-on-accent: {{colors.on_accent}};

  /* Muted text */
  --text-muted: color-mix(in srgb, {{colors.foreground}} 70%, transparent);
  --text-faint: color-mix(in srgb, {{colors.foreground}} 55%, transparent);

  /* Code */
  --code-normal: {{colors.color6}};

  /* Errors and success */
  --text-error: {{colors.color1}};
  --text-error-hover: {{colors.color1}};
  --text-success: {{colors.color2}};

  /* Tags */
  --tag-color: {{colors.color6}};
  --tag-background: {{colors.color8}};

  /* Graph */
  --graph-line: {{colors.color8}};
  --graph-node: {{colors.accent_ui}};
  --graph-node-focused: {{colors.color4}};
  --graph-node-tag: {{colors.color6}};
  --graph-node-attachment: {{colors.color2}};
}

/* Headers */
.cm-header-1, .markdown-rendered h1 { color: var(--text-title-h1); }
.cm-header-2, .markdown-rendered h2 { color: var(--text-title-h2); }
.cm-header-3, .markdown-rendered h3 { color: var(--text-title-h3); }
.cm-header-4, .markdown-rendered h4 { color: var(--text-title-h4); }
.cm-header-5, .markdown-rendered h5 { color: var(--text-title-h5); }
.cm-header-6, .markdown-rendered h6 { color: var(--text-title-h6); }

/* Code blocks */
.markdown-rendered code {
  color: {{colors.color6}};
}

/* Syntax highlighting */
.cm-s-obsidian span.cm-keyword { color: {{colors.color1}}; }
.cm-s-obsidian span.cm-string { color: {{colors.color2}}; }
.cm-s-obsidian span.cm-number { color: {{colors.color3}}; }
.cm-s-obsidian span.cm-comment { color: {{colors.color8}}; }
.cm-s-obsidian span.cm-operator { color: {{colors.color4}}; }
.cm-s-obsidian span.cm-def { color: {{colors.color4}}; }

/* Links */
.markdown-rendered a {
  color: var(--text-link);
}

/* Blockquotes */
.markdown-rendered blockquote {
  border-left-color: {{colors.accent_ui}};
}

/* Active elements */
.workspace-leaf.mod-active .workspace-leaf-header-title {
  color: var(--interactive-accent);
}

.nav-file-title.is-active {
  color: var(--interactive-accent);
}

/* Search results */
.search-result-file-title {
  color: var(--interactive-accent);
}
