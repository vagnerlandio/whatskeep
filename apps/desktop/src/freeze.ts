// Proteção contra poluição de protótipo (o que o `freezePrototype` do Tauri
// faz), mas só na janela principal. O `freezePrototype` global do Tauri vale
// para TODAS as janelas, inclusive a de login do Google, e quebra o
// JavaScript do Google ("Attempted to assign to readonly property"): o botão
// "Avançar" do login para de funcionar. Este módulo é o primeiro import de
// main.tsx, então roda antes de qualquer outro código do app.
Object.freeze(Object.prototype);

export {};
