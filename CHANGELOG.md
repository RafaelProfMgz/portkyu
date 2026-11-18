# Changelog

Todas as mudanças relevantes do projeto. Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).

## Não lançado

## v1.0.1 — 2026-11-18

### Adicionado
- Modo demonstração (`--demo`) com dados fictícios, para conhecer a interface e tirar capturas.
- `PORTKYU_SCREENSHOT=arquivo.png` salva uma captura da janela automaticamente.
- Guia de contribuição, código de conduta, política de segurança, modelos de issue e PR, README em inglês.
- CI com `cargo fmt --check` e `cargo clippy -D warnings`.

### Corrigido
- `portkyu -l | head` não gera mais panic de "broken pipe".
- A tabela não rola sozinha ao abrir, escondendo a primeira linha.

## v1.0.0 — 2026-11-18

Primeira versão.

- Interface gráfica nativa (egui) com teclado e mouse: tabela, painel de detalhes, menu de botão direito, busca e filtros.
- Interface de terminal (`--tui`) e modo lista (`-l`).
- Detecção de projeto, framework e branch do git a partir da pasta de cada processo.
- Containers Docker pela API (sem acordar o daemon parado).
- Classificação de exposição: local, bloqueada pelo ufw, liberada, exposta (incluindo Docker ignorando o ufw).
- Teste HTTP em segundo plano (status, tempo, servidor, título).
- Tráfego por porta com gráfico, clientes conectados e destinos do processo.
- Registro de atividade (portas que abrem e fecham).
- Ações: abrir no navegador, copiar URL, editor, terminal, logs do container, encerrar com confirmação.
- AppImage, tarball e instalador `install.sh`.
