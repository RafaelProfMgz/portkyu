# Política de segurança

## Versões com suporte

Correções de segurança são feitas sempre na versão mais recente publicada em
[Releases](https://github.com/RafaelProfMgz/portkyu/releases).

## Como relatar uma vulnerabilidade

**Não abra uma issue pública.** Use o relato privado do GitHub:

1. Acesse a aba **[Security → Report a vulnerability](https://github.com/RafaelProfMgz/portkyu/security/advisories/new)**.
2. Descreva o problema, como reproduzir e o impacto.

Você deve receber uma resposta em até 7 dias. Depois de corrigido, o relato vira um aviso público com os devidos
créditos (se você quiser).

## O que o PortKyu faz no seu sistema

Para facilitar a análise:

- **Lê** `/proc` (sockets e processos), `/etc/passwd` (nomes de usuário) e a configuração do `ufw` em `/etc/ufw`.
- **Executa** `ss` (tráfego por conexão) e `fc-match` (fontes). Ações do usuário podem executar `xdg-open`, o editor,
  o terminal, `docker logs` e `wl-copy`/`xclip`.
- **Conecta** apenas em `127.0.0.1`/`::1` nas portas abertas da própria máquina (teste HTTP `GET /`) e no socket
  Unix do Docker (`/var/run/docker.sock`), somente se o daemon já estiver rodando.
- **Envia sinais** (`SIGTERM`, depois `SIGKILL`) e para containers **somente** após confirmação explícita.
- **Não** faz nenhuma conexão com a internet, não coleta telemetria e não grava arquivos (exceto a captura opcional de
  `PORTKYU_SCREENSHOT`).
