# Superpowers Skills — Oxydactylus Development Toolkit

Este diretório contém um conjunto completo de skills (workflows estruturados) para desenvolvimento, planejamento, revisão de código e agentic development.

## 📚 Skills Disponíveis

| Skill | Descrição | Uso |
|-------|-----------|-----|
| **brainstorming** | Transforma ideias em designs aprovados via diálogo colaborativo | Antes de escrever código; refina requisitos |
| **dispatching-parallel-agents** | Executa tarefas independentes em paralelo com múltiplos subagents | Coordena múltiplas tarefas simultâneas |
| **executing-plans** | Executa planos de implementação task-by-task nesta sessão | Implementa um plano passo a passo |
| **finishing-a-development-branch** | Completa branches: merge, PR, ou discard com segurança | Ao terminar desenvolvimento de feature |
| **receiving-code-review** | Processa feedback de revisão de código | Quando receber comentários em PR |
| **requesting-code-review** | Solicita revisão especializada de código | Antes de merge/PR; valida qualidade |
| **subagent-driven-development** | Dispatch subagent por task com review completo | Implementação em larga escala; melhor qualidade |
| **systematic-debugging** | Metodologia estruturada para diagnosticar bugs | Ao investigar problemas |
| **test-driven-development** | Força workflow TDD: red → green → refactor | Desenvolvimento de features |
| **using-git-worktrees** | Gerencia git worktrees para branches isoladas | Trabalho paralelo em branches |
| **using-superpowers** | Introdução a como usar skills (this file) | Session start |
| **verification-before-completion** | Verifica que o trabalho está completo antes de finalizar | Antes de declarar task completa |
| **writing-plans** | Escreve planos de implementação detalhados | Após spec aprovado |
| **writing-skills** | Cria novas skills estruturadas | Ao criar workflows reutilizáveis |

## 🚀 Quick Start

### 1. Para Planejar Uma Feature

```
brainstorming → writing-plans → subagent-driven-development
```

- **brainstorming**: Refina a ideia colaborativamente
- **writing-plans**: Gera plano de 10+ tasks
- **subagent-driven-development**: Executa com subagent por task + review

### 2. Para Debugar Um Bug

```
systematic-debugging → fix code → verification-before-completion
```

### 3. Para Implementar Um Plano Existente

Escolha uma abordagem:

**Subagent-Driven (recomendado):**
- Melhor qualidade (review automática por task)
- Mais rápido (paralelização entre tasks)
- Usa: `subagent-driven-development`

**Inline (mesma sessão):**
- Mantém contexto local
- Menos subagents
- Usa: `executing-plans`

### 4. Para Finalizar Uma Branch

```
verification-before-completion → finishing-a-development-branch
```

## 📖 Como Usar Skills

### Em Claude Code (CLI ou Desktop)

```bash
# Skills carregam automaticamente com o prefixo /
/brainstorming        # Carrega a skill de brainstorming
/writing-plans        # Carrega a skill de writing-plans
```

Ou use o Skill tool:

```python
Skill(skill="brainstorming", args="...")
```

### Em Outras IDEs/CLIs

Copie a skill relevante de `docs/superpowers/skills/<skill-name>/` para seu ambiente.

## 🔧 Estrutura de Uma Skill

Cada skill é um diretório com:

- **README.md** (ou primeira seção) — overview e processo
- **Prompts** — templates para subagents (`.md` ou `.txt`)
- **Scripts** — ferramentas auxiliares (em `scripts/`)
- **References** — documentação técnica

Exemplo: `brainstorming/`
```
brainstorming/
  README.md (conteúdo principal)
  visual-companion.md (guia para UI companion)
  references/
    ...
```

## 🎯 Princípios de Design

Todas as skills seguem estes princípios:

1. **Explícito > Implícito** — Workflows estruturados, não sugestões vagas
2. **TDD + Frequent Commits** — Red → Green → Refactor → Commit
3. **Spec-Driven** — Especificação escrita antes de implementação
4. **Fresh Context per Task** — Subagents recebem exatamente o que precisam
5. **Self-Review + External Review** — Implementer self-reviews; reviewer valida
6. **YAGNI Rigoroso** — Apenas o que está no spec

## 📝 Estrutura de Planos

Planos criados por `writing-plans` seguem este padrão:

```markdown
# Feature Name Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development

**Goal:** [Uma frase]
**Architecture:** [2-3 frases]
**Tech Stack:** [Key tech]

## Global Constraints
[Requisitos vinculantes do projeto]

### Task 1: [Component]
**Files:** [Arquivos a tocar]
**Interfaces:** [Consumes/Produces]
- [ ] Step 1: [Ação específica + código completo]
- [ ] Step 2: [Teste que falha]
...
```

## 🧪 Fluxos de Desenvolvimento Recomendados

### Feature Nova (Grande)

1. ✅ Brainstorm (refinar ideia)
2. ✅ Especificação escrita + aprovada
3. ✅ Plano detalhado (10+ tasks)
4. ✅ Subagent-Driven Development
5. ✅ Final code review (requesting-code-review)
6. ✅ Merge + publicação

### Bug Fix (Pequeno)

1. ✅ systematic-debugging (isolar raiz)
2. ✅ TDD: write failing test
3. ✅ Fix code
4. ✅ verification-before-completion
5. ✅ Commit + PR

### Refator Existente

1. ✅ writing-plans (especificar mudanças)
2. ✅ subagent-driven-development (com review)
3. ✅ Final review
4. ✅ Merge

## 🔗 Integração com Oxydactylus

Oxydactylus já usa superpowers para:

- **Plan 1-6**: Implementados via `subagent-driven-development`
- **Plan 7** (atual): `writing-plans` gerou docs/superpowers/plans/2026-06-25-ownership-subusers.md
- **Futuro**: Mais features via mesma pipeline

**Próximo passo:** Executar Plan 7 com `subagent-driven-development`

## 📖 Referências

- **CLAUDE.md** — Contexto do projeto Oxydactylus
- **docs/superpowers/specs/** — Especificações de features
- **docs/superpowers/plans/** — Planos de implementação
- **`.superpowers/sdd/progress.md`** — Ledger de progresso SDD

---

**Última atualização:** 2026-06-25  
**Skills versão:** 6.0.3
