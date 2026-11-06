# Automação: motor, linha de comando e MCP

[English](automation.md)

Tudo o que o ConstructCAD faz num modelo pode ser feito sem a interface. O **motor** (`cc_engine`) reúne as operações num catálogo de **comandos** nomeados, e três clientes usam esse mesmo catálogo:

```
                     ┌──────────────────────────────┐
  aplicativo     ──▶│                              │
  ccad (CLI)     ──▶│  cc_engine: Session +        │──▶ o modelo (.ccad)
  agentes (MCP)  ──▶│  catálogo de comandos        │
  scripts Lua    ──▶│  (+ comandos de plugins)     │
                     └──────────────────────────────┘
```

Uma parede desenhada com o mouse, um `ccad run wall_create …`, um script Lua chamando `ccad.run("wall_create", …)` e um agente de IA chamando a ferramenta `wall_create` executam o mesmo código, com os mesmos padrões, verificações e desfazer. A lista completa de comandos e parâmetros está na [referência de comandos](commands.md) (em inglês, gerada do próprio catálogo); [plugins](#plugins) acrescentam seus próprios comandos ao catálogo.

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
| `script` | um script ou plugin Lua falhou: erro de sintaxe ou de execução, ou um limite atingido |

## Linha de comando: `ccad`

O `ccad` é um programa separado, sem dependências gráficas (roda em servidores e na CI). Ele vem nos pacotes de release; a partir do código-fonte: `cargo build --release -p cc_cli` (o binário é `target/release/ccad`).

```text
ccad [opções] commands             lista os comandos
ccad [opções] schema <comando>     o esquema JSON dos parâmetros de um comando
ccad reference                     a referência de comandos em Markdown
ccad [opções] run <comando> [args] executa um comando
ccad [opções] script <arquivo|->   executa um script de comandos
ccad [opções] lua <arquivo|-> [args] executa um script Lua
ccad [opções] plugins              lista os plugins carregados com --plugins
ccad plugins install <zip> <pasta>  instala um arquivo de plugin numa pasta de plugins
ccad plugins remove <id> <pasta>    remove um plugin instalado
ccad [opções] mcp                  serve os comandos por MCP (stdio)

-p, --project <arquivo>   abre um projeto antes (.ccad, ou .ifc para importar)
--save                    salva o projeto no fim (no próprio arquivo)
--save-as <arquivo>       salva o projeto no fim em <arquivo>
--plugins <pasta>         carrega o plugin da <pasta>, ou cada plugin das suas
                          subpastas (pode repetir)
--lang <código>           idioma dos nomes padrão e mensagens (também CCAD_LANG):
                          en, pt-BR, es, fr, de, it, lo, pl, uk-UA, zh-TW, zh-CN ou ja
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
ccad -p casa.ccad run view_export view="Térreo" path=terreo.dxf             # ou em DXF, em tamanho real em mm
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

### Terreno

`topography_create` cria uma superfície de terreno a partir de pontos medidos `[x, y, z]` (cotas acima do nível) dentro de um contorno `boundary` ou `curves`; sem nenhum dos dois, o contorno convexo dos pontos o limita. Os vértices do contorno recebem as cotas que os pontos lhes dão, e a terra fecha até uma base plana `base_depth` (2 m por padrão) abaixo do ponto mais baixo, então os cortes a cortam como um sólido. `element_get` mostra as cotas mínima e máxima, a base, a área, o volume e os avisos (`few-points`, `collinear`, `boundary-crossing`) em `derived.topography`. `topography_points_set` substitui os pontos ou altera, remove e adiciona pontos pelo índice; `sketch_set` redesenha o contorno e, com `points`, substitui os pontos no mesmo passo. Um tipo de terreno define o material da terra e o intervalo e as mestras das curvas de nível (`topography_type_create`, `type_set`).

```text
{"command": "topography_create", "args": {"points": [[0, 0, 0], [20, 0, 2], [20, 10, 2.5], [0, 10, 0.5], [10, 5, 1.2]]}}
```

Scripts não têm variáveis: um passo não usa o id criado por um passo anterior. Os comandos ajudam aceitando posições onde isso importa (`door_create` e `window_create` encontram a parede mais próxima de `point`; pisos, forros e telhados acompanham as paredes com `from_walls`). Para lógica entre passos, escreva um [script Lua](#scripts-lua), chame `ccad run` a partir de um script de shell ou use um agente via MCP.

## Scripts Lua

`ccad lua <arquivo>` executa um script em [Lua 5.4](https://www.lua.org/manual/5.4/) sobre a sessão. O script chama comandos com `ccad.run`, guarda os resultados em variáveis e faz laços, então o id criado num passo pode ser usado no seguinte. Os argumentos (nas mesmas formas do `run`: pares `chave=valor`, um objeto JSON, `@arquivo.json`) ficam em `ccad.args`; o que ele imprime vai para a saída padrão e o que ele retorna é impresso como JSON.

[`examples/small_house.lua`](../examples/small_house.lua) constrói a casa pequena acima, com as medidas dos argumentos:

```lua
local w = ccad.args.width or 8
local d = ccad.args.depth or 6
local corners = { { 0, 0 }, { w, 0 }, { w, d }, { 0, d } }
local outer = {}
for i = 1, 4 do
  outer[i] = ccad.run("wall_create", { start = corners[i], ["end"] = corners[i % 4 + 1], height = 2.8 }).id
end
ccad.run("door_create", { wall = outer[1], offset = w / 4 })
…
```

```sh
ccad --save-as casa.ccad lua examples/small_house.lua width=10 depth=7
```

A tabela `ccad`:

| | |
|---|---|
| `ccad.run(nome, args)` | executa um comando e devolve o resultado como tabela Lua; gera um erro se ele falhar |
| `ccad.try(nome, args)` | o mesmo, mas devolve `nil, err` na falha (`err.code`, `err.message`; veja [Erros](#conceitos)) |
| `ccad.commands()` | os comandos: `name`, `kind`, `group` e `description` de cada um |
| `ccad.describe(nome)` | um comando, com o esquema JSON dos parâmetros em `schema` |
| `ccad.args` | os argumentos do script (também `...`) |
| `ccad.null` | o `null` do JSON (o `nil` do Lua omite o campo) |
| `ccad.array(t)` | marca `t` como lista, para que uma tabela vazia vá como `[]` e não `{}` |
| `ccad.json.encode(v)`, `ccad.json.decode(s)` | texto JSON de e para valores Lua |
| `ccad.version`, `ccad.api` | a versão do ConstructCAD e a da API de plugins (1) |

Argumentos e resultados são JSON: objetos são tabelas com chaves de texto, listas são sequências (`{ 0, 0 }` é um ponto), e `end`, palavra reservada do Lua, se escreve `["end"]`. Um número inteiro calculado com `/` (que em Lua sempre dá um número real) vai como inteiro, então `10 / 2` funciona onde se espera um id ou uma contagem.

**O sandbox.** Os scripts rodam num Lua restrito: as bibliotecas `table`, `string`, `math`, `utf8` e `coroutine` e as funções básicas, sem `io`, `os`, `debug`, `package`, `require`, `load`, `dofile` nem `loadfile`. Um script só chega ao projeto pelos comandos, e a arquivos só pelos comandos que os leem ou gravam. Cada execução tem um limite de 500 milhões de instruções Lua e 256 MB de memória; passando deles, ela para com um erro `script` (capturar esse erro com `pcall` não faz o script continuar). Cada comando que um script executa é um passo de desfazer.

## Plugins

Um plugin é uma pasta com um manifesto `plugin.toml` e código Lua que acrescenta comandos ao catálogo. Os comandos dele são listados e executados como os nativos, pelo `ccad run`, por scripts, por scripts Lua e por agentes MCP, e se chamam `<id do plugin>__<comando>` (dois sublinhados, que os nomes nativos nunca têm).

```sh
ccad --plugins examples/plugins commands                # o catálogo, com os comandos dos plugins
ccad --plugins examples/plugins plugins                 # os plugins, as permissões e os comandos deles
ccad --plugins examples/plugins run quick_rooms__rectangle_room width=4 depth=3 name=Escritório door=south
claude mcp add constructcad -- ccad mcp --plugins /caminho/para/plugins
```

`--plugins` recebe a pasta de um plugin ou uma pasta de pastas de plugins, e pode ser repetido. Um plugin que não carrega é informado na saída de erro e fica de fora; o que os plugins imprimem também vai para a saída de erro.

### Escrevendo um plugin

[`examples/plugins/quick_rooms`](../examples/plugins/quick_rooms) é um exemplo completo. `plugin.toml`:

```toml
id = "quick_rooms"          # letras minúsculas, dígitos e sublinhados simples, até 32
name = "Quick rooms"
version = "0.1.0"
api = 1                     # a API de plugins para a qual ele foi escrito
description = "Rectangular rooms in one step, and wall totals by type."
authors = ["…"]
license = "GPL-3.0-or-later"
homepage = "https://…"
main = "main.lua"           # o padrão
permissions = []            # ou ["files"], veja abaixo
```

`main.lua` roda uma vez quando o plugin carrega e registra comandos com `ccad.command`:

```lua
local rect = require("lib.rect")   -- lib/rect.lua na pasta do plugin

ccad.command {
  name = "rectangle_room",         -- o comando é quick_rooms__rectangle_room
  title = "Rectangular room",
  description = "Four walls around a rectangle, its floor and its room.",
  kind = "edit",                   -- "query", "edit" (o padrão) ou "file"
  params = {                       -- um esquema JSON dos argumentos
    properties = { width = { type = "number" }, depth = { type = "number" } },
    required = { "width", "depth" },
  },
  run = function(args)
    local p = rect.corners(0, 0, args.width, args.depth)
    for i = 1, 4 do ccad.run("wall_create", { start = p[i], ["end"] = p[i % 4 + 1] }) end
    return { room = ccad.run("room_create", { point = { args.width / 2, args.depth / 2 } }).id }
  end,
}
```

- `run` recebe os argumentos numa tabela e devolve o resultado do comando (uma tabela, um valor ou nada). Argumentos `required` que faltam são recusados antes de ele rodar; `error("mensagem", 0)` faz o comando falhar com essa mensagem.
- **Um passo de desfazer:** quantos comandos um comando de plugin executar, ele é desfeito de uma vez, e quando falha (um erro, um `ccad.run` que falhou, um limite atingido) tudo o que ele fez é desfeito.
- **O que um comando pode executar:** um comando `query` só consulta; um `edit` consulta e edita; comandos que leem ou gravam arquivos (importar, exportar, salvar) precisam da permissão `files`, e só com ela se declara um comando do tipo `file`. Abrir ou criar um projeto, `undo`, `redo` e `language_set` não rodam dentro de um comando de plugin, nem o comando de outro plugin.
- **O sandbox** é o dos [scripts Lua](#scripts-lua), com o limite contado por comando; cada plugin tem o seu próprio estado Lua, que guarda as variáveis entre um comando e outro. `require("a.b")` carrega `a/b.lua` da pasta do plugin (uma vez; arquivos fora da pasta são recusados), e `ccad.plugin` tem o `id`, o `name` e a `version` dele. `ccad.run` só funciona enquanto um comando roda, não enquanto o plugin carrega.
- **As permissões** são concedidas por quem carrega o plugin: `ccad --plugins` concede o que o manifesto pede, já que você indicou a pasta.

Os plugins são compartilhados pelo índice da comunidade, [constructcad-plugins](https://github.com/francisreboucas/constructcad-plugins): uma entrada por plugin, com de onde baixar cada versão e o checksum dela, enviada por pull request. No aplicativo, *Gerenciar → Plugins* instala, atualiza, desativa e remove plugins ([Plugins](interface.pt-BR.md#plugins)); sem ele, `ccad plugins install <arquivo.zip> <pasta>` instala um arquivo baixado (verificado do mesmo jeito: caminhos seguros, tamanhos e um plugin que carrega) e `ccad plugins remove <id> <pasta>` remove um.

## Componentes com script

Uma peça de componente pode ser um **script**: código Lua que declara os parâmetros e os hotspots do componente e monta as peças a partir dos valores deles. O script é puro: ele vê os valores e nada mais (nem comandos, nem arquivos, nem o modelo), então roda uma vez para cada conjunto de valores; mover ou girar um componente já colocado não o roda de novo. [`examples/components/cabinet.lua`](../examples/components/cabinet.lua) é um exemplo completo.

```lua
param.length   { "depth", label = "Profundidade", default = 0.55 }      -- compartilhado pelos elementos do tipo
param.choice   { "handle", { { "bar", "Barra" }, { "none", "Nenhum" } } }
param.length   { "width", label = "Largura", default = 0.8, instance = true }  -- próprio de cada elemento
param.boolean  { "worktop", default = true, instance = true }
param.material { "body", label = "Corpo", instance = true }
hotspot        { "plinth", 0, 0.05, 0, label = "Rodapé", shape = "diamond", color = "#ff8800" }

function geometry(p)
  local pieces = { box { 0, 0, 0, p.width, p.depth, 0.85, material = p.body } }
  if p.worktop then pieces[#pieces + 1] = box { 0, 0, 0.85, p.width, p.depth + 0.02, 0.03 } end
  return pieces
end
```

| Declaração | Valor em `p` |
|---|---|
| `param.length`, `param.angle`, `param.number { id, label=, default=0, instance= }` | um número (metros, graus) |
| `param.boolean { id, default=false }` | `true` ou `false` |
| `param.text { id, default="" }` | um texto |
| `param.choice { id, { {chave, rótulo}, … }, default=primeira chave }` | a chave escolhida |
| `param.material { id, default="" }` | o nome de um material do projeto (vazio: o material da peça) |
| `hotspot { id, x, y, z, label=, shape="circle", color="#00ffff" }` | `{ x, y, z }`, onde o elemento o tem |

As declarações ficam no corpo do script, nunca dentro de `geometry`. Os ids são nomes (letras, dígitos, `_`) e não se repetem; mudar um rótulo não renomeia o parâmetro. Um parâmetro sem `instance = true` é do tipo, e mudá-lo muda todos os elementos do tipo; com ele, cada elemento guarda o seu valor. As formas de hotspot são `circle`, `square`, `triangle`, `diamond` e `triangle_down`; as cores, `#RRGGBB` ou `#RRGGBBAA`. `p` também traz os outros parâmetros do componente, seus planos de referência e, em portas e janelas, `WallThickness`.

`geometry(p)` devolve uma lista de peças (listas de peças são achatadas), no referencial do componente (metros a partir do ponto de inserção, x para a direita, y para trás, z para cima). Cada construtor recebe os valores em ordem ou por nome, além de `name`, `material`, `void`, `in_plan`, `plan_only` e `detail` (uma lista de `coarse`, `medium`, `fine`):

| Construtor | Valores |
|---|---|
| `box` | `x, y, z, width, depth, height` |
| `cylinder` | `x, y, z, radius, height` |
| `extrusion` | `points` (`{x, y}` em planta), `z, height` |
| `profile` | `points` (`{u, v}`, ou `{u, v, radius = r}` para um arco até o próximo ponto), `offset, depth`, `plane = "plan"`, `"front"` ou `"side"` |
| `revolution` | `points, offset, axis, angle` (padrão 360), `plane` |
| `sweep` | `path, profile, offset`, `plane` |
| `blend` | `bottom, top, offset, depth`, `plane` |
| `mesh` | `vertices` (`{x, y, z}`), `faces` (listas de números de vértice a partir de 1, no sentido anti-horário vistas de fora) |

Os scripts rodam no [sandbox](#scripts-lua) com limites menores (20 milhões de instruções, 64 MB, 10.000 peças, um milhão de pontos), e `math.random` repete os mesmos números. Quando um script falha, o elemento mantém as últimas peças que funcionaram, e o erro, com a linha, aparece em `element_get` (`derived.script_errors`), na paleta de propriedades e em `component_params_get`; corrigir o script ou o valor o recupera.

Comandos:

```sh
ccad run component_create name=Armario 'def={"items": [{"name": "Script", "shape": {"Script": {"source": ""}}}]}'
ccad run component_script_set type=Armario piece=Script source=@examples/components/cabinet.lua
ccad run component_params_get type=Armario
ccad run component_params_set type=Armario name=handle value='"none"'
ccad run component_params_set 'elements=[100, 101]' name=width value=1.2
ccad run component_hotspot_move element=100 name=plinth 'point=[0, 0.08, 0]'
```

- `component_script_set` grava uma peça de script (ou acrescenta uma). Um script que não carrega (erro de sintaxe, declaração inválida) é recusado; as declarações viram os parâmetros e hotspots do componente, os que continuam declarados mantêm os valores quando o tipo ainda serve, e o resultado lista os erros da primeira execução. Um componente tem uma peça de script; seus parâmetros mudam no script, não com `component_param_set` ou `component_param_delete`.
- `component_params_get` mostra os parâmetros e hotspots de um tipo, ou de componentes colocados de um mesmo componente, com `mixed` onde diferem.
- `component_params_set` define o valor de um tipo, ou o valor de instância de vários elementos de uma vez: números, `true`/`false`, textos, a chave de uma opção ou o nome de um material. É um passo de desfazer, e nada muda quando algum elemento recusa o valor.
- `component_hotspot_move` move um hotspot no referencial do componente, ou em coordenadas do projeto com `world=true`.

Um material citado por um parâmetro conta como usado: não pode ser excluído, e renomeá-lo renomeia os valores. A exportação IFC grava as peças que o script fez, como em qualquer componente.

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

Comandos acrescentados durante a execução implementam `cc_engine::Extension` (`crates/cc_engine/src/extension.rs`) e são postos na sessão com `Session::set_extension`; `cc_plugins::PluginHost` é a dos plugins Lua. `Session::commands`, `describe` e `kind_of` cobrem os dois, e `Session::execute` roda um comando de extensão como um passo de desfazer (`Document::squash_since`).

Os detalhes de arquitetura estão em [AGENTS.md](../AGENTS.md).
