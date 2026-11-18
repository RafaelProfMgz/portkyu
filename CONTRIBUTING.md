# Contribuindo com o PortKyu

Obrigado por querer ajudar! 💜 Toda contribuição é bem-vinda: relatar bug, sugerir ideia, melhorar a documentação,
traduzir ou mandar código. *English speakers: issues and PRs in English are welcome too.*

## Antes de começar

- **Bug ou ideia?** Abra uma [issue](https://github.com/RafaelProfMgz/portkyu/issues/new/choose) usando o modelo.
- **Dúvida ou conversa?** Use as [Discussions](https://github.com/RafaelProfMgz/portkyu/discussions).
- **Mudança grande?** Abra uma issue primeiro para combinarmos a abordagem antes de você investir tempo.
- Procurando por onde começar? Veja as issues com a etiqueta
  [`good first issue`](https://github.com/RafaelProfMgz/portkyu/labels/good%20first%20issue).

## Ambiente de desenvolvimento

Você precisa de Linux e do [Rust](https://rustup.rs) estável.

```bash
git clone https://github.com/RafaelProfMgz/portkyu.git
cd portkyu
cargo run --release -- --demo   # interface com dados fictícios (não precisa de Docker nem servidores)
cargo run --release             # dados reais da sua máquina
cargo run --release -- --tui    # interface de terminal
```

O modo `--demo` é o jeito mais rápido de mexer na interface: tem servidores dev, bancos, containers, tráfego e uma
porta exposta, sem depender do que está rodando na sua máquina. Nele, as ações (encerrar, abrir editor…) ficam
desativadas.

### Arquitetura

| Arquivo | Responsabilidade |
|---|---|
| `src/collect.rs` | sockets e processos lidos do `/proc` |
| `src/project.rs` | detecção de projeto, framework e branch do git |
| `src/docker.rs` | API do Docker pelo socket Unix |
| `src/firewall.rs` | estado do `ufw` e classificação de exposição |
| `src/probe.rs` | teste HTTP em uma thread separada |
| `src/traffic.rs` | bytes por conexão (`ss -ti`) |
| `src/app.rs` | estado, classificação, eventos e ações — **não depende de nenhuma interface** |
| `src/gui.rs` | interface gráfica (egui/eframe) |
| `src/tui.rs`, `src/ui.rs` | interface de terminal (ratatui) |

Regra de ouro: lógica nova vai em `app.rs` (ou num módulo de coleta) e as duas interfaces só desenham.

### Ideias que ajudariam muito

- Detectar mais frameworks em `src/project.rs` (Rails, Spring, Laravel, Deno Fresh…).
- Suporte a `firewalld` e `nftables` além do `ufw` em `src/firewall.rs`.
- Podman além do Docker.
- Empacotamento: Flatpak, AUR, `.deb`, `.rpm`.
- Tradução da interface para inglês (i18n).

## Enviando um pull request

1. Faça um fork e crie uma branch: `git checkout -b feat/minha-ideia`.
2. Garanta que passa nas mesmas verificações do CI:
   ```bash
   cargo fmt
   cargo clippy --release -- -D warnings
   cargo build --release
   ```
3. Se mexer na interface, teste com `--demo` e, se puder, anexe uma captura no PR.
   Para gerar uma captura automática: `PORTKYU_SCREENSHOT=captura.png cargo run --release -- --demo`.
4. Use mensagens de commit claras (português ou inglês). O padrão
   [Conventional Commits](https://www.conventionalcommits.org/pt-br/) (`feat:`, `fix:`, `docs:`…) é bem-vindo.
5. Abra o PR descrevendo **o quê** e **por quê**. Atualize o `CHANGELOG.md` na seção "Não lançado".

## Publicando uma versão (mantenedores)

1. Atualize `version` no `Cargo.toml` e o `CHANGELOG.md`.
2. `git commit -am "release: vX.Y.Z" && git tag vX.Y.Z && git push --follow-tags`
3. O workflow `release` gera o AppImage e o tarball e publica em Releases.

## Código de conduta

Este projeto segue o nosso [Código de Conduta](CODE_OF_CONDUCT.md). Ao participar, você concorda em respeitá-lo.
