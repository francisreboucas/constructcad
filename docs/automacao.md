# Automação: motor, linha de comando e MCP

[English](automation.md)

Tudo o que o ConstructCAD faz num modelo pode ser feito sem a interface. O **motor** (`cc_engine`) reúne as operações num catálogo de **comandos** nomeados, e três clientes usam esse mesmo catálogo:

```
                     ┌──────────────────────────────┐
  aplicativo     ──▶│                              │
  ccad (CLI)     ──▶│  cc_engine: Session +        │──▶ o modelo (.ccad)
  agentes (MCP)  ──▶│  catálogo de comandos        │
                     └──────────────────────────────┘
```

Uma parede desenhada com o mouse, um `ccad run wall_create …` e um agente de IA chamando a ferramenta `wall_create` executam o mesmo código, com os mesmos padrões, verificações e desfazer. A lista completa de comandos e parâmetros está na [referência de comandos](commands.md) (em inglês, gerada do próprio catálogo).

## Conceitos

**Sessão.** Um projeto aberto: o documento (com o histórico de desfazer) e a geometria e os desenhos derivados dele. A CLI abre uma por execução (ou script), o servidor MCP uma por conexão, o aplicativo uma por janela.

**Comandos.** Cada comando tem um nome em `snake_case`, um objeto JSON de parâmetros descrito por um esquema JSON e um resultado JSON. Tipos:

| Tipo | Faz | Exemplos |
|---|---|---|
| query | lê o modelo | `elements_list`, `element_get`, `schedule_table` |
| edit | altera o modelo: um passo de desfazer | `wall_create`, `elements_move`, `element_set` |
| file | trabalha com arquivos ou substitui o projeto | `project_open`, `ifc_import`, `sheet_export` |

**Unidades e coordenadas.** Comprimentos em metros, ângulos em graus, pontos `[x, y]` em planta. As cotas usam as coordenadas do desenho da sua vista (X/Y da planta nas plantas; posição horizontal e altura nas elevações e cortes). Áreas e volumes nos resultados vêm em m² e m³.

**Referências.** Níveis, tipos, vistas e materiais são indicados por **id ou nome** (`"level": 3` ou `"level": "Térreo"`; maiúsculas e minúsculas não importam; um nome ambíguo dá erro pedindo o id). Folhas também podem ser indicadas pelo número (`"A101"`). Elementos são indicados por id, que `elements_list` e os resultados dos comandos de criação informam. Sem nível, vale o nível mais baixo; sem tipo, o tipo padrão da categoria.

**Ler e alterar parâmetros.** `element_get`, `type_get` e `view_get` mostram os `params` de um objeto; `element_set`, `type_set` e `view_set` recebem os campos a alterar no mesmo formato e os mesclam:

```sh
ccad -p casa.ccad run element_get id=42
ccad -p casa.ccad --save run element_set id=42 'params={"height": 2.7}'
ccad -p casa.ccad --save run element_set id=57 'params={"shape": {"Gable": {"slope": 0.5, "ridge_deg": 0}}}'
```

Objetos se mesclam campo a campo; listas e valores simples substituem; `null` limpa um valor opcional. `material_set` e `units_set` (no formato de `material_get` e `units_get`) funcionam do mesmo jeito.

**Gráficos das vistas.** O que uma vista mostra e como faz parte da vista: `view_graphics_get` lista a visibilidade e as sobreposições de cada categoria, os filtros aplicados e as sobreposições de elementos; `view_graphics_set` mostra ou oculta uma categoria e sobrepõe suas linhas em projeção e de corte (cor, espessura em mm de papel, padrão), preenchimentos de superfície e de corte (cor, padrão), meio-tom e transparência; `element_override_set` e `element_hide_in_view` fazem o mesmo para elementos numa vista. As sobreposições são objetos como `{"cut_lines": {"color": [255, 0, 0], "weight": 0.5, "pattern": "Dash"}, "cut_fill": {"pattern": "Solid", "color": [200, 200, 200]}, "halftone": true, "transparency": 0.4}` e se mesclam como `params` (`null` redefine um atributo, `reset: true` recomeça). Valem em plantas, plantas de forro, elevações, cortes, vistas 3D, folhas e exportações PDF/SVG.

```sh
ccad -p casa.ccad --save run view_graphics_set view=Térreo category=door visible=false
ccad -p casa.ccad --save run filter_create name=Longas 'categories=["wall"]' 'rules=[{"param": "length", "op": "greater", "value": 5}]'
ccad -p casa.ccad --save run view_filter_add view=Térreo filter=Longas 'overrides={"cut_lines": {"color": [255, 0, 0]}}'
ccad -p casa.ccad run filter_elements filter=Longas
```

*Filtros* (`filter_create`, `filter_set`, `filter_delete`, `filters_list`, `filter_elements`) selecionam elementos por categoria e regras sobre parâmetros — qualquer campo de tabela (`type`, `mark`, `comments`, `level`, `material`, `system`, `structural`, `width`, `height`, `thickness`, `length`, `area`, `volume`, `diameter`…) ou `elevation` (do nível do elemento) — com `equals`, `not_equals`, `contains`, `not_contains`, `begins_with`, `not_begins_with`, `greater`, `greater_or_equal`, `less`, `less_or_equal`, `has_value` e `has_no_value`, todas as regras (padrão) ou qualquer uma (`any: true`). Uma regra sobre um campo que nenhuma das categorias do filtro tem (`width` em paredes) é recusada, pois nunca passaria. Quantidades são comparadas em unidades do SI (metros); texto, sem diferenciar maiúsculas. Uma vista aplica filtros em ordem (`view_filter_add`, `view_filter_set`, `view_filter_move`, `view_filter_remove`): o primeiro filtro que corresponde vence em cada atributo, as sobreposições do próprio elemento vencem os filtros e os filtros vencem a categoria; um elemento fica oculto se estiver oculto na vista, se a categoria estiver desligada ou se algum filtro correspondente o ocultar.

*Modelos de vista* (`template_create` a partir de uma vista, `template_set`, `template_delete`, `templates_list`) guardam escala, gráficos das categorias, filtros, altura de corte das plantas, exibição de instalações (sobre a arquitetura esmaecida) e exibição estrutural, cada um com uma marca `include`. `view_template_apply` copia-os para vistas uma vez; `view_template_assign` faz as vistas seguirem o modelo: as propriedades incluídas são copiadas agora e a cada mudança do modelo (no mesmo passo de desfazer), e editá-las na vista é recusado. Excluir um modelo libera as vistas, que mantêm suas propriedades.

*Ocultar/isolar temporário* (`view_temporary` com `action` `hide`, `isolate`, `hide_category`, `isolate_category` ou `reset`) é estado de exibição da sessão, não uma mudança do modelo: não é salvo, não entra no desfazer e não é impresso (folhas e exportações mostram a vista inteira). `view_temporary_apply` o torna permanente (oculto na vista, categorias desligadas) como um passo de desfazer.

**Fases de obra.** Uma reforma é modelada em fases (`phases_list`; o modelo tem *Existente* e *Construção nova*): cada elemento do modelo é construído numa fase (`phase_created`) e pode ser demolido na mesma ou numa posterior (`phase_demolished`), como `element_get` mostra. `phase_create` (no fim ou antes de outra, `before`), `phase_set` (renomear, ou mover com `index`; uma mudança que demoliria elementos antes de construí-los é recusada) e `phase_delete` (uma fase em uso precisa de `merge_into`) as gerenciam. Elementos novos são construídos na fase de criação da sessão (`phase_creation_set`; a da vista ativa no aplicativo, senão a última), cópias também. `elements_phase_set` define `created` e `demolished` (`null`: de pé) e `elements_demolish` demole numa fase (`phase`) ou na fase de uma vista (`view`). Portas, janelas e armaduras seguem seus hospedeiros: nunca são mais antigas que eles e são demolidas com eles, a menos que demolidas antes. Cotas, textos e eixos não têm fase.

```sh
ccad -p casa.ccad --save run elements_phase_set 'ids=[12, 13]' created=Existente
ccad -p casa.ccad --save run elements_demolish 'ids=[13]' view=Térreo
ccad -p casa.ccad --save run view_set view=Térreo 'phase_filter=Mostrar anterior + demolição'
ccad -p casa.ccad run phases_list
```

Cada planta, elevação, corte, vista 3D e tabela mostra o modelo na sua fase (`phase`) com um filtro de fases (`phase_filter`, ambos com `view_set`; modelos de vista controlam o filtro, não a fase). Em relação à fase da vista um elemento é *existente* (construído antes, de pé), *demolido* (nesta fase), *novo* (construído nesta fase) ou *temporário* (construído e demolido nela); elementos construídos depois ou demolidos antes não aparecem. Os filtros de fases (`phase_filters_list`, `phase_filter_create`, `phase_filter_set`, `phase_filter_delete`; o modelo traz *Mostrar tudo*, *Mostrar completo*, *Mostrar novo*, *Mostrar anterior + demolição*, *Mostrar anterior + novo*, *Mostrar fase anterior* e *Mostrar demolição + novo*) dizem, para cada situação, `not_displayed`, `by_category` ou `overridden`, que usa os gráficos da situação definidos no projeto (`phase_graphics_get`, `phase_graphics_set`: existente esmaecido, demolido tracejado e sem hachura de corte, temporário esmaecido e tracejado), abaixo das sobreposições de elementos e dos filtros de vista e acima da categoria. Uma vista cuja fase deixa elementos de fora tem geometria própria: portas de fases posteriores não abrem paredes existentes e paredes só se unem com paredes da fase. Um ambiente é delimitado pelas paredes de pé ao fim da fase em que é criado, então paredes demolidas nessa fase ou antes não o delimitam. Tabelas listam o que sua fase e seu filtro mostram e oferecem os campos `PhaseCreated` e `PhaseDemolished` (que também servem em regras de filtros, como `phase_created` e `phase_demolished`).

**Desenhos CAD importados.** `cad_import` lê um arquivo DWG (R12 até 2018 e posteriores, pela biblioteca acadrust) ou DXF (ASCII ou binário, lido pelo próprio ConstructCAD) para uma planta, planta de forro, elevação, corte ou legenda, num passo de desfazer, e informa o que leu: `format`, `version`, `units` (e `units_assumed`), `layers`, `entities`, `blocks`, `primitives`, `skipped` (entidades não desenhadas, por tipo), `truncated` e a extensão na vista (`extents`). Blocos são inseridos (aninhados, escalados, girados, em matriz), arcos e círculos continuam arcos, elipses, splines e contornos de hachuras viram polilinhas, textos e MTEXT mantêm posição, altura e rotação; sólidos 3D, imagens, linhas infinitas e entidades parecidas são ignorados. `position` com `align` posiciona a origem do desenho (`origin`, o padrão), o centro da sua extensão (`center`) ou a origem, a menos que fique a mais de 1 km do conteúdo da vista (`auto`); `units` substitui as do arquivo (`mm`, `cm`, `dm`, `m`, `km`, `in`, `ft`, `yd`, `mi`), e arquivos sem unidades são lidos em metros quando pequenos, senão em milímetros. O elemento começa fixado e só a sua vista o desenha, sob o modelo (categoria `cadimport`; vistas 3D e exportação IFC o deixam de fora). `element_get` mostra o posicionamento em `params` (`units`, `scale`, `position`, `rotation_deg`, `mirrored`, `hidden_layers`, `color_mode` `Grey`/`Layers`/`Black`, `halftone`), mas não o desenho; `element_set` os altera; os comandos de mover, girar, espelhar, fixar e excluir valem para ele. `cad_layers` lista as camadas e `cad_layers_set` as mostra ou oculta.

```sh
ccad -p casa.ccad --save run cad_import path=levantamento.dwg view=Térreo align=auto
ccad -p casa.ccad --save run element_set id=120 'params={"units": "Centimeters", "halftone": true}'
ccad -p casa.ccad --save run cad_layers_set id=120 'layers=["COTAS", "TEXTOS"]' visible=false
```

**Validação e desfazer.** Cada edição é um passo de desfazer (`undo`, `redo`). Uma edição que deixaria o modelo inválido — uma porta numa parede inexistente, uma medida negativa, uma referência a nada — é recusada sem deixar rastro. Elementos fixados não são movidos, aparados nem excluídos.

**Erros.** Um comando que falha devolve `{"error": {"code": …, "message": …}}`, com a mensagem no idioma da sessão (`--lang`, `language_set`).

| Código | Significado |
|---|---|
| `unknown_command` | nenhum comando tem esse nome |
| `invalid_arguments` | os parâmetros não seguem o esquema ou não fazem sentido (comprimento zero, tipo desconhecido…) |
| `not_found` | o elemento, tipo, nível, vista ou material indicado não existe |
| `rejected` | a edição deixaria o modelo inválido ou não é permitida (fixado, em uso…) |
| `io` | falha ao ler ou gravar um arquivo |
| `internal` | um defeito; o projeto fica como estava |

## Linha de comando: `ccad`

O `ccad` é um programa separado, sem dependências gráficas (roda em servidores e na CI). Ele vem nos pacotes de release; a partir do código-fonte: `cargo build --release -p cc_cli` (o binário é `target/release/ccad`).

```text
ccad [opções] commands             lista os comandos
ccad [opções] schema <comando>     o esquema JSON dos parâmetros de um comando
ccad reference                     a referência de comandos em Markdown
ccad [opções] run <comando> [args] executa um comando
ccad [opções] script <arquivo|->   executa um script de comandos
ccad [opções] mcp                  serve os comandos por MCP (stdio)

-p, --project <arquivo>   abre um projeto antes (.ccad, ou .ifc para importar)
--save                    salva o projeto no fim (no próprio arquivo)
--save-as <arquivo>       salva o projeto no fim em <arquivo>
--lang <código>           idioma dos nomes padrão e mensagens (também CCAD_LANG):
                          en, pt-BR, es, fr, de, it, lo, pl, uk-UA, zh-TW ou zh-CN
```

Os argumentos de `run` podem ser um objeto JSON, `@arquivo.json`, `-` (entrada padrão) ou pares `chave=valor`, cujos valores são lidos como JSON quando possível e como texto caso contrário:

```sh
ccad -p casa.ccad --save run wall_create start=[0,0] end=[5,0] height=2.8
ccad -p casa.ccad --save run wall_create start=[5,0] end=[5,4] mid=[6,2]    # parede em arco passando por mid
ccad -p casa.ccad run elements_list category=door
ccad -p casa.ccad --save run view_set '{"view": "Corte 1", "scale": 100}'
ccad -p modelo.ifc --save-as modelo.ccad run project_info          # importa um IFC
ccad -p casa.ccad run sheet_export sheet=A101 path=a101.pdf
ccad -p casa.ccad run view_export view="Corte 1" path=corte.png dpi=300     # qualquer vista em PNG, SVG ou PDF
```

Os resultados saem como JSON na saída padrão. Erros vão para a saída de erro como JSON, com código de saída 1; erros de uso saem com 2. Sem `--save` ou `--save-as`, o arquivo do projeto não muda.

Os resultados se combinam com outras ferramentas; por exemplo, com `jq`:

```sh
parede=$(ccad -p casa.ccad --save run wall_create start=[0,0] end=[6,0] | jq .id)
ccad -p casa.ccad --save run door_create wall=$parede offset=1.5
```

### Scripts

Um script é uma lista de passos: um array JSON de objetos `{"command": …, "args": {…}}` ou um desses objetos por linha (linhas que começam com `//` são comentários). `ccad script` executa os passos em ordem numa mesma sessão e para no primeiro erro; cada passo é um passo de desfazer.

[`examples/small_house.ccmacro`](../examples/small_house.ccmacro) constrói uma casa pequena:

```text
// A small house built with engine commands only.
{"command": "wall_create", "args": {"start": [0, 0], "end": [8, 0], "height": 2.8}}
…
{"command": "door_create", "args": {"point": [2, -0.5]}}
{"command": "floor_create", "args": {"from_walls": true}}
{"command": "room_create", "args": {"point": [2, 3], "name": "Living room"}}
```

```sh
ccad --save-as casa.ccad script examples/small_house.ccmacro
```

### Contornos desenhados com curvas

Pisos, forros, telhados, radiers e massas recebem o contorno como `curves` em vez de pontos: linhas, arcos (por `bulge` ou por um ponto `through` por onde passam), círculos, elipses (inteiras ou parte delas), retângulos e polígonos regulares (estes dois viram linhas). Laços dentro do contorno são aberturas de pisos e telhados. O elemento guarda as curvas: `sketch_get` as devolve (ou linhas ao longo do contorno de um elemento desenhado por pontos) e `sketch_set` desenha o contorno de novo; mover, girar e espelhar o elemento leva as curvas junto, e editar diretamente os pontos do contorno as descarta.

```text
{"command": "mass_create", "args": {"curves": [{"kind": "circle", "center": [0, 0], "radius": 5}], "height": 9}}
{"command": "floor_create", "args": {"curves": [{"kind": "rectangle", "corner": [0, 0], "opposite": [10, 8]}, {"kind": "circle", "center": [5, 4], "radius": 1}]}}
```

Scripts não têm variáveis: um passo não usa o id criado por um passo anterior. Os comandos ajudam aceitando posições onde isso importa (`door_create` e `window_create` encontram a parede mais próxima de `point`; pisos, forros e telhados acompanham as paredes com `from_walls`). Para lógica entre passos, chame `ccad run` a partir de um script de shell ou use um agente via MCP.

## Macros no aplicativo

*Gerenciar → Macros → Gravar macro* grava as alterações que você fizer até apertar *Parar gravação*; a macro é salva como script (`.ccmacro`, o formato acima). *Executar macro* roda uma macro ou qualquer script no projeto aberto; `ccad script` roda os mesmos arquivos. Com uma pasta de macros definida em *Preferências → Locais de arquivos*, os diálogos de arquivo de macro começam nela e *Executar macro* vira um menu dos arquivos `.ccmacro` dela (mais *Procurar…* para qualquer outro arquivo).

- Só edições são gravadas: abrir, salvar e exportar ficam de fora, então reproduzir nunca sobrescreve arquivos.
- Um valor arrastado nas propriedades é gravado uma vez, com o valor final; alterações feitas num diálogo cancelado não são gravadas.
- Os passos gravados se referem a elementos pelo id. No projeto em que foi gravada (ou numa cópia dele como estava no início da gravação), a macro reproduz as mesmas alterações; em outro projeto, passos que indicam elementos pelo id podem falhar ou atingir outros elementos.

## Paleta de comandos no aplicativo

**Ctrl+Shift+P** (ou a lupa no fim das abas do ribbon) abre a paleta de comandos. Ela encontra comandos da interface, vistas para abrir ou criar e os comandos do motor pelo nome, pela descrição ou pelo atalho, sem diferenciar acentos nem maiúsculas. Comandos do motor abrem um formulário com os argumentos em JSON, já com os campos obrigatórios; **Executar** (Ctrl+Enter) os roda no projeto aberto, como um passo de desfazer, e mostra o resultado. Os comandos de arquivo (`project_open`, `ifc_export`…) ficam na aba *Arquivo*.

A dica de cada botão do ribbon mostra o comando do motor que ele executa, para que o que você faz à mão possa virar um script.

## Servidor MCP

`ccad mcp` é um servidor [Model Context Protocol](https://modelcontextprotocol.io/) na entrada/saída padrão. Cada comando é uma ferramenta com o mesmo nome, descrição e esquema de entrada; as consultas são marcadas como somente leitura. O servidor mantém uma sessão, então um agente abre ou cria um projeto, trabalha nele ao longo de várias chamadas e o salva.

**Claude Code:**

```sh
claude mcp add constructcad -- ccad mcp
# ou já com um projeto aberto
claude mcp add constructcad -- ccad mcp -p /caminho/para/casa.ccad
```

**Claude Desktop e outros clientes** (`claude_desktop_config.json` ou as configurações MCP do cliente; use o caminho completo do `ccad`):

```json
{
  "mcpServers": {
    "constructcad": { "command": "/caminho/para/ccad", "args": ["mcp", "--lang", "pt-BR"] }
  }
}
```

Exemplos de pedidos que um agente resolve com essas ferramentas: *"abra casa.ccad, liste as portas sem marca e numere-as"*, *"importe modelo.ifc e diga o volume das paredes de concreto"*, *"crie um pavimento superior 3 m acima do térreo com as mesmas paredes externas"*, *"exporte a folha A101 em PDF"*.

Observações:

- Os caminhos são da máquina onde o `ccad` roda.
- Erros voltam como resultado da ferramenta com `isError` e o objeto de erro, para que o modelo corrija a chamada.
- O servidor fala as revisões 2025-06-18, 2025-03-26 e 2024-11-05 do protocolo e é implementado em `crates/cc_cli/src/mcp.rs` (JSON-RPC 2.0, uma mensagem por linha).

## Para desenvolvedores: criar um comando

Toda funcionalidade de usuário nasce como comando do motor; o aplicativo chama o comando em vez de alterar o documento.

1. Escreva a estrutura de parâmetros em `crates/cc_engine/src/commands/<área>.rs`: `#[derive(Debug, Deserialize, Serialize, JsonSchema)]`, `#[serde(deny_unknown_fields)]`, um comentário de documentação (a descrição do comando) e um em cada campo (a descrição do parâmetro; um teste falha sem eles). O código e os comentários são em inglês.
2. Implemente `Command` com `NAME` (`snake_case`), `KIND` e `run`. Resolva referências com `refs`, altere o modelo dentro de `commands::edit` (um passo de desfazer, recusado se o resultado precisar de reparo) e devolva JSON (`created(s, id)` para elementos). Use mensagens `t!` para erros que pessoas vão ler.
3. Registre-o no seu grupo em `registry.rs`.
4. Teste-o em `crates/cc_engine/tests/commands.rs` por `Session::execute`.
5. Gere a referência de novo: `cargo run -p cc_cli -- reference > docs/commands.md` (um teste falha enquanto ela estiver desatualizada).
6. No aplicativo, chame-o com `Session::run(Comando { … })`, `actions::run_command` ou os auxiliares de `engine_edit` (use `Session::run_merged` para valores arrastados). Isso também o torna gravável em macros.

Os detalhes de arquitetura estão em [AGENTS.md](../AGENTS.md).
