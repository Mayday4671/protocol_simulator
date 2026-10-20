import "monaco-editor/nls/lang/zh-cn";
import * as monaco from "monaco-editor/editor/editor.api";
import EditorWorker from "monaco-editor/editor/editor.worker?worker";
import "monaco-editor/languages/definitions/xml/register";
import "monaco-editor/features/codeEditor/register";
import "monaco-editor/features/find/register";
import "monaco-editor/features/folding/register";
import "monaco-editor/features/bracketMatching/register";
import "monaco-editor/features/clipboard/register";
import "monaco-editor/features/contextmenu/register";
import "monaco-editor/features/gotoLine/register";
import "monaco-editor/features/gotoError/register";
import "monaco-editor/features/hover/register";
import "monaco-editor/features/indentation/register";
import "monaco-editor/features/wordOperations/register";
import "monaco-editor/features/linesOperations/register";
import "monaco-editor/features/comment/register";

// Packaged locally with Vite; the installed desktop editor never uses a CDN.
self.MonacoEnvironment = { getWorker: () => new EditorWorker() };

monaco.editor.defineTheme("simterm-xml-light", {
  base: "vs", inherit: true,
  rules: [
    { token: "tag.xml", foreground: "0369A1" },
    { token: "delimiter.xml", foreground: "0369A1" },
    { token: "attribute.name.xml", foreground: "92400E" },
    { token: "attribute.value.xml", foreground: "047857" },
    { token: "comment.xml", foreground: "71717A" },
  ],
  colors: {
    "editor.background": "#FFFFFF", "editor.foreground": "#18181B",
    "editorLineNumber.foreground": "#71717A", "editorLineNumber.activeForeground": "#18181B",
    "editor.lineHighlightBackground": "#F4F4F5", "editorGutter.background": "#FAFAFA",
    "editorWidget.background": "#FAFAFA", "editorWidget.border": "#D4D4D8",
  },
});
monaco.editor.defineTheme("simterm-xml-dark", {
  base: "vs-dark", inherit: true,
  rules: [
    { token: "tag.xml", foreground: "7DD3FC" },
    { token: "delimiter.xml", foreground: "7DD3FC" },
    { token: "attribute.name.xml", foreground: "FCD34D" },
    { token: "attribute.value.xml", foreground: "6EE7B7" },
    { token: "comment.xml", foreground: "A1A1AA" },
  ],
  colors: {
    "editor.background": "#09090B", "editor.foreground": "#F4F4F5",
    "editorLineNumber.foreground": "#A1A1AA", "editorLineNumber.activeForeground": "#F4F4F5",
    "editor.lineHighlightBackground": "#18181B", "editorGutter.background": "#101012",
    "editorWidget.background": "#18181B", "editorWidget.border": "#3F3F46",
  },
});

export { monaco };
